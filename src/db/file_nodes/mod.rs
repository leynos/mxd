//! File-node repository helpers and recursive tree traversal.

use diesel::{
    OptionalExtension,
    QueryableByName,
    prelude::*,
    query_builder::QueryFragment,
    result::QueryResult,
    sql_query,
    sql_types::{BigInt, Integer, Nullable, Text},
};
use diesel_async::RunQueryDsl;
use diesel_cte_ext::{
    RecursiveParts,
    builders,
    cte::{RecursiveBackend, WithRecursive},
};

use super::connection::{Backend, DbConnection};
#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
use super::insert::fetch_last_insert_rowid;
use crate::models::{FileNode, NewFileNode, NewResourcePermission};

/// Resource type label stored for file-node ACL rows.
pub const FILE_NODE_RESOURCE_TYPE: &str = "file_node";

/// Principal type label stored for direct user grants.
pub const USER_PRINCIPAL_TYPE: &str = "user";

cfg_if::cfg_if! {
    if #[cfg(feature = "postgres")] {
        const DESCENDANT_SEED_SQL: &str = concat!(
            "SELECT id, parent_id, name, node_type, 0 AS depth ",
            "FROM file_nodes WHERE id = $1"
        );
    } else {
        const DESCENDANT_SEED_SQL: &str = concat!(
            "SELECT id, parent_id, name, node_type, 0 AS depth ",
            "FROM file_nodes WHERE id = ?"
        );
    }
}

const DESCENDANT_STEP_SQL: &str = concat!(
    "SELECT child.id AS id, child.parent_id AS parent_id, child.name AS name, ",
    "child.node_type AS node_type, tree.depth + 1 AS depth ",
    "FROM tree JOIN file_nodes child ON child.parent_id = tree.id"
);
const DESCENDANT_BODY_SQL: &str = concat!(
    "SELECT id, parent_id, name, node_type, depth ",
    "FROM tree ORDER BY depth ASC, name ASC"
);

/// A file-node row returned from the recursive descendant query.
#[derive(Debug, PartialEq, Eq, QueryableByName)]
pub struct FileNodeDescendant {
    /// File-node identifier.
    #[diesel(sql_type = Integer)]
    pub id: i32,
    /// Parent folder identifier, or `None` for the sentinel root.
    #[diesel(sql_type = Nullable<Integer>)]
    pub parent_id: Option<i32>,
    /// Visible name within the parent folder.
    #[diesel(sql_type = Text)]
    pub name: String,
    /// Node kind (`file`, `folder`, or `alias`).
    #[diesel(sql_type = Text)]
    pub node_type: String,
    /// Depth relative to the recursive query root.
    #[diesel(sql_type = BigInt)]
    pub depth: i64,
}

fn build_descendant_cte<Seed, Step, Body>(
    seed: Seed,
    step: Step,
    body: Body,
) -> WithRecursive<Backend, (), Seed, Step, Body>
where
    Backend: RecursiveBackend + diesel::backend::DieselReserveSpecialization,
    Seed: QueryFragment<Backend>,
    Step: QueryFragment<Backend>,
    Body: QueryFragment<Backend>,
{
    builders::with_recursive::<Backend, (), _, _, _, _>(
        "tree",
        &["id", "parent_id", "name", "node_type", "depth"],
        RecursiveParts::new(seed, step, body),
    )
}

/// Load the sentinel root file-node row.
///
/// # Errors
///
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn get_root_file_node(conn: &mut DbConnection) -> QueryResult<FileNode> {
    use crate::schema::file_nodes::dsl as f;

    f::file_nodes
        .filter(f::is_root.eq(true))
        .get_result(conn)
        .await
}

#[cfg(any(feature = "postgres", feature = "returning_clauses_for_sqlite_3_35"))]
async fn create_file_node_inner(
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

#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
async fn create_file_node_inner(
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

/// Insert a new file tree node.
///
/// # Errors
///
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn create_file_node(conn: &mut DbConnection, node: &NewFileNode<'_>) -> QueryResult<i32> {
    create_file_node_inner(conn, node).await
}

/// Add a resource-scoped permission grant.
///
/// # Errors
///
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn add_resource_permission(
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

/// Find a direct child node by parent and name.
///
/// # Errors
///
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn find_child_file_node(
    conn: &mut DbConnection,
    parent_id: i32,
    name: &str,
) -> QueryResult<Option<FileNode>> {
    use crate::schema::file_nodes::dsl as f;

    f::file_nodes
        .filter(f::parent_id.eq(parent_id))
        .filter(f::name.eq(name))
        .get_result(conn)
        .await
        .optional()
}

/// List direct child nodes under a folder.
///
/// # Errors
///
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn list_child_file_nodes(
    conn: &mut DbConnection,
    parent_id: i32,
) -> QueryResult<Vec<FileNode>> {
    use crate::schema::file_nodes::dsl as f;

    f::file_nodes
        .filter(f::parent_id.eq(parent_id))
        .order((f::node_type.asc(), f::name.asc()))
        .load(conn)
        .await
}

/// List direct child nodes with explicit grants for the given user.
///
/// # Errors
///
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn list_permitted_child_file_nodes_for_user(
    conn: &mut DbConnection,
    parent_id: i32,
    user_id: i32,
) -> QueryResult<Vec<FileNode>> {
    use crate::schema::{file_nodes::dsl as f, resource_permissions::dsl as p};

    f::file_nodes
        .inner_join(
            p::resource_permissions.on(p::resource_id
                .eq(f::id)
                .and(p::resource_type.eq(FILE_NODE_RESOURCE_TYPE))
                .and(p::principal_type.eq(USER_PRINCIPAL_TYPE))
                .and(p::principal_id.eq(user_id))
                .and(p::privileges.gt(0_i64))),
        )
        .filter(f::parent_id.eq(parent_id))
        .order((f::node_type.asc(), f::name.asc()))
        .select((
            f::id,
            f::is_root,
            f::node_type,
            f::name,
            f::parent_id,
            f::alias_target_id,
            f::object_key,
            f::size,
            f::comment,
            f::is_dropbox,
            f::created_at,
            f::updated_at,
            f::created_by,
        ))
        .load(conn)
        .await
}

/// Recursively list a subtree including the starting node.
///
/// # Errors
///
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn list_descendant_file_nodes(
    conn: &mut DbConnection,
    root_id: i32,
) -> QueryResult<Vec<FileNodeDescendant>> {
    let seed = sql_query(DESCENDANT_SEED_SQL).bind::<Integer, _>(root_id);
    let step = sql_query(DESCENDANT_STEP_SQL);
    let body = sql_query(DESCENDANT_BODY_SQL);
    let query = build_descendant_cte(seed, step, body);

    query.load(conn).await
}

#[cfg(test)]
mod tests;
