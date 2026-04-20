//! File hierarchy and ACL helpers.

use cfg_if::cfg_if;
use diesel::{
    OptionalExtension,
    QueryableByName,
    prelude::*,
    result::QueryResult,
    sql_query,
    sql_types::{Integer, Nullable, Text},
};
use diesel_async::RunQueryDsl;
use thiserror::Error;

use super::connection::DbConnection;
#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
use super::insert::fetch_last_insert_rowid;
use crate::{
    file_path::{FILE_NODE_BODY_SQL, FILE_NODE_STEP_SQL, build_path_cte_with_conn, prepare_path},
    models::{
        FileNode,
        NewFileNode,
        NewGroup,
        NewPermission,
        NewResourcePermission,
        NewUserGroup,
        VisibleFileNode,
    },
};

const RESOURCE_TYPE_FILE_NODE: &str = "file_node";
const PRINCIPAL_USER: &str = "user";
const PRINCIPAL_GROUP: &str = "group";
const DOWNLOAD_FILE_PERMISSION_CODE: i32 = 2;

/// Errors that can occur when resolving file-node paths.
#[derive(Debug, Error)]
pub enum FileNodeLookupError {
    /// The provided file path is invalid or malformed.
    #[error("invalid file path")]
    InvalidPath,
    /// A database query error occurred.
    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),
    /// A JSON serialisation error occurred while preparing the path.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// Insert a permission row if needed and return its identifier.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn seed_permission(
    conn: &mut DbConnection,
    permission: &NewPermission<'_>,
) -> QueryResult<i32> {
    use crate::schema::permissions::dsl as p;

    diesel::insert_into(p::permissions)
        .values(permission)
        .on_conflict(p::code)
        .do_nothing()
        .execute(conn)
        .await?;

    p::permissions
        .filter(p::code.eq(permission.code))
        .select(p::id)
        .first(conn)
        .await
}

/// Insert a group row if needed and return its identifier.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn create_group(conn: &mut DbConnection, group: &NewGroup<'_>) -> QueryResult<i32> {
    use crate::schema::groups::dsl as g;

    diesel::insert_into(g::groups)
        .values(group)
        .on_conflict(g::name)
        .do_nothing()
        .execute(conn)
        .await?;

    g::groups
        .filter(g::name.eq(group.name))
        .select(g::id)
        .first(conn)
        .await
}

/// Link a user to a group.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn add_user_to_group(
    conn: &mut DbConnection,
    membership: &NewUserGroup,
) -> QueryResult<bool> {
    use crate::schema::user_groups::dsl::user_groups;

    diesel::insert_into(user_groups)
        .values(membership)
        .on_conflict_do_nothing()
        .execute(conn)
        .await
        .map(|rows| rows > 0)
}

cfg_if! {
    if #[cfg(any(feature = "postgres", feature = "returning_clauses_for_sqlite_3_35"))] {
        /// Insert a new file node and return its identifier.
        ///
        /// # Errors
        /// Returns any error produced by the database.
        #[must_use = "handle the result"]
        pub async fn create_file_node(
            conn: &mut DbConnection,
            node: &NewFileNode<'_>,
        ) -> QueryResult<i32> {
            use crate::schema::file_nodes::dsl::{file_nodes, id};

            diesel::insert_into(file_nodes)
                .values(node)
                .returning(id)
                .get_result(conn)
                .await
        }
    } else if #[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))] {
        /// Insert a new file node and return its identifier.
        ///
        /// # Errors
        /// Returns any error produced by the database.
        #[must_use = "handle the result"]
        pub async fn create_file_node(
            conn: &mut DbConnection,
            node: &NewFileNode<'_>,
        ) -> QueryResult<i32> {
            use crate::schema::file_nodes::dsl::file_nodes;

            diesel::insert_into(file_nodes)
                .values(node)
                .execute(conn)
                .await?;
            fetch_last_insert_rowid(conn).await
        }
    } else {
        compile_error!("Either 'sqlite' or 'postgres' feature must be enabled");
    }
}

/// Grant a resource-scoped permission to a principal.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn grant_resource_permission(
    conn: &mut DbConnection,
    permission: &NewResourcePermission<'_>,
) -> QueryResult<bool> {
    use crate::schema::resource_permissions::dsl::resource_permissions;

    diesel::insert_into(resource_permissions)
        .values(permission)
        .on_conflict_do_nothing()
        .execute(conn)
        .await
        .map(|rows| rows > 0)
}

async fn file_node_id_from_path(
    conn: &mut DbConnection,
    path: &str,
) -> Result<Option<i32>, FileNodeLookupError> {
    #[derive(QueryableByName)]
    struct NodeId {
        #[diesel(sql_type = Nullable<Integer>)]
        id: Option<i32>,
    }

    let Some((json, len)) = prepare_path(path)? else {
        return Ok(None);
    };

    let step = sql_query(FILE_NODE_STEP_SQL).bind::<Text, _>(json);
    let len_i32 = i32::try_from(len).map_err(|_| FileNodeLookupError::InvalidPath)?;
    let body = sql_query(FILE_NODE_BODY_SQL).bind::<Integer, _>(len_i32);
    let query = build_path_cte_with_conn(conn, step, body);
    let result: Option<NodeId> = query.get_result(conn).await.optional()?;
    Ok(result.and_then(|row| row.id))
}

/// Fetch a single file node by identifier.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn get_file_node(conn: &mut DbConnection, node_id: i32) -> QueryResult<Option<FileNode>> {
    use crate::schema::file_nodes::dsl as f;

    f::file_nodes
        .filter(f::id.eq(node_id))
        .first::<FileNode>(conn)
        .await
        .optional()
}

/// Resolve a file path to its terminal file node.
///
/// # Errors
/// Returns an error if the path is malformed or if the lookup fails.
#[must_use = "handle the result"]
pub async fn resolve_file_node_path(
    conn: &mut DbConnection,
    path: &str,
) -> Result<Option<FileNode>, FileNodeLookupError> {
    let Some(node_id) = file_node_id_from_path(conn, path).await? else {
        return Ok(None);
    };
    get_file_node(conn, node_id)
        .await
        .map_err(FileNodeLookupError::from)
}

/// List the child nodes directly beneath the selected parent.
///
/// Pass `None` to list the top-level namespace.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn list_child_file_nodes(
    conn: &mut DbConnection,
    parent_id: Option<i32>,
) -> QueryResult<Vec<FileNode>> {
    use crate::schema::file_nodes::dsl as f;

    let query = parent_id.map_or_else(
        || f::file_nodes.into_boxed().filter(f::parent_id.is_null()),
        |selected_parent_id| {
            f::file_nodes
                .into_boxed()
                .filter(f::parent_id.eq(selected_parent_id))
        },
    );

    query.order(f::name.asc()).load::<FileNode>(conn).await
}

/// Resolve the alias target for a file node, if one exists.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn resolve_alias_target(
    conn: &mut DbConnection,
    node_id: i32,
) -> QueryResult<Option<FileNode>> {
    use crate::schema::file_nodes::dsl as f;

    let target_id = f::file_nodes
        .filter(f::id.eq(node_id))
        .select(f::alias_target_id)
        .first::<Option<i32>>(conn)
        .await
        .optional()?
        .flatten();

    match target_id {
        Some(alias_target_id) => get_file_node(conn, alias_target_id).await,
        None => Ok(None),
    }
}

/// List the visible top-level file nodes for the selected user.
///
/// Visibility is defined as a resource permission granting protocol privilege
/// code `2` (*Download File*) either directly to the user or indirectly via
/// one of their groups.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn list_visible_root_file_nodes_for_user(
    conn: &mut DbConnection,
    user_id: i32,
) -> QueryResult<Vec<VisibleFileNode>> {
    use crate::schema::{
        file_nodes::dsl as f,
        permissions::dsl as p,
        resource_permissions::dsl as rp,
        user_groups::dsl as ug,
    };

    let group_ids = ug::user_groups
        .filter(ug::user_id.eq(user_id))
        .select(ug::group_id);

    f::file_nodes
        .inner_join(
            rp::resource_permissions.on(rp::resource_type
                .eq(RESOURCE_TYPE_FILE_NODE)
                .and(rp::resource_id.eq(f::id))),
        )
        .inner_join(p::permissions.on(p::id.eq(rp::permission_id)))
        .filter(f::parent_id.is_null())
        .filter(p::code.eq(DOWNLOAD_FILE_PERMISSION_CODE))
        .filter(
            rp::principal_type
                .eq(PRINCIPAL_USER)
                .and(rp::principal_id.eq(user_id))
                .or(rp::principal_type
                    .eq(PRINCIPAL_GROUP)
                    .and(rp::principal_id.eq_any(group_ids))),
        )
        .select((f::id, f::name, f::kind))
        .distinct()
        .order(f::name.asc())
        .load::<VisibleFileNode>(conn)
        .await
}
