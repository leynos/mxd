//! Shared file-node test bodies and `PostgreSQL` harness helpers.

#[cfg(any(feature = "sqlite", feature = "postgres"))]
use diesel::prelude::*;
#[cfg(feature = "postgres")]
use diesel_async::AsyncConnection;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use diesel_async::RunQueryDsl;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use test_util::AnyError;

mod additional;
mod shared;
mod shared_core;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
mod sqlite;
pub(super) use shared::{
    file_node_check_kind_constraint_body,
    nested_child_not_visible_without_explicit_grant_body,
    non_download_permission_does_not_grant_visibility_body,
    resolve_file_node_path_returns_none_for_missing_path_body,
};
pub(super) use shared_core::{
    file_node_acl_flow_body,
    group_acl_visibility_body,
    resolve_file_node_path_and_alias_body,
};
#[cfg(feature = "sqlite")]
pub(super) use shared_core::{
    grant_revocation_removes_visibility_body,
    group_membership_removal_revokes_visibility_body,
};
#[cfg(feature = "postgres")]
pub(super) use sqlite::visible_root_files_merge_body;

#[cfg(any(feature = "sqlite", feature = "postgres"))]
use crate::{
    db::{
        DbConnection,
        create_file_node,
        create_group,
        create_user,
        download_file_permission,
        get_user_by_name,
        grant_resource_permission,
        seed_permission,
    },
    models::{FileNodeKind, NewFileNode, NewGroup, NewResourcePermission, NewUser},
    schema::file_nodes::dsl as file_nodes,
};

/// Seed the canonical `download_file` permission and return its ID.
///
/// # Errors
///
/// Propagates any database error.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(super) async fn seed_download_permission(conn: &mut DbConnection) -> Result<i32, AnyError> {
    seed_permission(conn, &download_file_permission())
        .await
        .map_err(anyhow::Error::from)
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn create_test_user(conn: &mut DbConnection, username: &str) -> Result<i32, AnyError> {
    create_user(
        conn,
        &NewUser {
            username,
            password: "hash",
        },
    )
    .await?;

    get_user_by_name(conn, username)
        .await?
        .map(|user| user.id)
        .ok_or_else(|| anyhow::anyhow!("test user '{username}' missing after insert"))
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
struct RootFileNodeSpec<'a> {
    name: &'a str,
    object_key: &'a str,
    size: i64,
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn create_root_file_node_for_owner(
    conn: &mut DbConnection,
    owner_id: i32,
    spec: RootFileNodeSpec<'_>,
) -> Result<i32, AnyError> {
    create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: spec.name,
            parent_id: None,
            alias_target_id: None,
            object_key: Some(spec.object_key),
            size: Some(spec.size),
            comment: None,
            is_dropbox: false,
            creator_id: owner_id,
        },
    )
    .await
    .map_err(anyhow::Error::from)
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn resource_permission_count(conn: &mut DbConnection) -> Result<i64, AnyError> {
    use crate::schema::resource_permissions::dsl as resource_permissions;

    resource_permissions::resource_permissions
        .count()
        .get_result::<i64>(conn)
        .await
        .map_err(anyhow::Error::from)
}

/// Run `f` against a freshly migrated `PostgreSQL` database.
///
/// Uses `POSTGRES_TEST_URL` when set; otherwise starts embedded `PostgreSQL`.
///
/// # Errors
/// Returns any setup, migration, closure, or shutdown error.
#[cfg(feature = "postgres")]
pub(super) async fn with_embedded_pg<F>(db_name: &str, f: F) -> Result<(), AnyError>
where
    F: for<'conn> FnOnce(
        &'conn mut DbConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), AnyError>> + 'conn>,
    >,
{
    use postgresql_embedded::PostgreSQL;

    if std::env::var_os("POSTGRES_TEST_URL").is_some() {
        let db = test_util::postgres::PostgresTestDb::new_async()
            .await
            .map_err(anyhow::Error::from)?;
        crate::db::run_migrations(db.url.as_ref(), None)
            .await
            .map_err(anyhow::Error::from)?;
        let mut conn = diesel_async::AsyncPgConnection::establish(db.url.as_ref())
            .await
            .map_err(anyhow::Error::from)?;
        return f(&mut conn).await;
    }

    let mut pg = PostgreSQL::default();
    pg.setup().await.map_err(anyhow::Error::from)?;
    pg.start().await.map_err(anyhow::Error::from)?;

    let result = async {
        pg.create_database(db_name)
            .await
            .map_err(anyhow::Error::from)?;
        let url = pg.settings().url(db_name);
        crate::db::run_migrations(&url, None)
            .await
            .map_err(anyhow::Error::from)?;
        let mut conn = diesel_async::AsyncPgConnection::establish(&url)
            .await
            .map_err(anyhow::Error::from)?;
        f(&mut conn).await
    }
    .await;

    let stop_result = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(anyhow::Error::from)?;
        runtime.block_on(pg.stop()).map_err(anyhow::Error::from)
    })
    .join()
    .map_err(|_| anyhow::anyhow!("embedded postgres shutdown thread panicked"))?;
    result.and(stop_result)
}

/// Verify that the database rejects an update that would make a node its own
/// parent.
///
/// # Errors
/// Returns an error if the constraint was not enforced or a database operation
/// failed unexpectedly.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(super) async fn reject_self_parent_body(
    conn: &mut DbConnection,
    check_msg: &str,
) -> Result<(), AnyError> {
    let owner_id = create_test_user(conn, "selfparent").await?;

    let folder_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::Folder.as_str(),
            name: "Loop",
            parent_id: None,
            alias_target_id: None,
            object_key: None,
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id: owner_id,
        },
    )
    .await?;

    let Err(err) = diesel::update(file_nodes::file_nodes.filter(file_nodes::id.eq(folder_id)))
        .set(file_nodes::parent_id.eq(Some(folder_id)))
        .execute(conn)
        .await
    else {
        anyhow::bail!("self-parent update should fail");
    };
    anyhow::ensure!(
        err.to_string()
            .to_lowercase()
            .contains(&check_msg.to_lowercase()),
        "self-parent update returned unexpected error: {err}"
    );
    Ok(())
}

/// Verify that the database rejects file nodes with invalid basenames.
///
/// # Errors
///
/// Returns an error if an invalid basename was accepted or a database operation
/// failed unexpectedly.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(super) async fn reject_invalid_basenames_body(
    conn: &mut DbConnection,
    check_msg: &str,
) -> Result<(), AnyError> {
    let owner_id = create_test_user(conn, "basename-owner").await?;

    for (index, invalid_name) in ["", "bad/name"].into_iter().enumerate() {
        let object_key = format!("objects/invalid-name-{index}.txt");
        let Err(err) = create_file_node(
            conn,
            &NewFileNode {
                kind: FileNodeKind::File.as_str(),
                name: invalid_name,
                parent_id: None,
                alias_target_id: None,
                object_key: Some(object_key.as_str()),
                size: Some(1),
                comment: None,
                is_dropbox: false,
                creator_id: owner_id,
            },
        )
        .await
        else {
            anyhow::bail!("invalid basename '{invalid_name}' should be rejected");
        };
        anyhow::ensure!(
            err.to_string()
                .to_lowercase()
                .contains(&check_msg.to_lowercase()),
            "invalid basename '{invalid_name}' returned unexpected error: {err}"
        );
    }

    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn grant_cleanup_permissions(conn: &mut DbConnection) -> Result<(i32, i32), AnyError> {
    let owner_id = create_test_user(conn, "owner").await?;
    let grantee_id = create_test_user(conn, "grantee").await?;
    let group_id = create_group(conn, &NewGroup { name: "cleanup" }).await?;
    let permission_id = seed_download_permission(conn).await?;
    let node_id = create_root_file_node_for_owner(
        conn,
        owner_id,
        RootFileNodeSpec {
            name: "cleanup.txt",
            object_key: "objects/cleanup.txt",
            size: 9,
        },
    )
    .await?;

    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "user",
            principal_id: grantee_id,
            permission_id,
        },
    )
    .await?;
    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "group",
            principal_id: group_id,
            permission_id,
        },
    )
    .await?;

    Ok((grantee_id, group_id))
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn delete_cleanup_principals(
    conn: &mut DbConnection,
    grantee_id: i32,
    group_id: i32,
) -> Result<(), AnyError> {
    use crate::schema::{groups::dsl as groups_dsl, users::dsl as users_dsl};

    diesel::delete(users_dsl::users.filter(users_dsl::id.eq(grantee_id)))
        .execute(conn)
        .await?;
    diesel::delete(groups_dsl::groups.filter(groups_dsl::id.eq(group_id)))
        .execute(conn)
        .await?;

    Ok(())
}

/// Verify that deleting a user or group cascades to `resource_permissions`.
///
/// # Errors
///
/// Returns an error if the ACL count was unexpected or a database operation
/// failed.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(super) async fn cleanup_on_principal_delete_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    let initial_count = resource_permission_count(conn).await?;
    anyhow::ensure!(
        initial_count == 0,
        "cleanup test expected empty ACL table at start, found {initial_count} rows"
    );

    let (grantee_id, group_id) = grant_cleanup_permissions(conn).await?;
    let granted_count = resource_permission_count(conn).await?;
    anyhow::ensure!(
        granted_count == 2,
        "cleanup test expected 2 ACL rows after grants, found {granted_count}"
    );

    delete_cleanup_principals(conn, grantee_id, group_id).await?;

    let remaining_count = resource_permission_count(conn).await?;
    anyhow::ensure!(
        remaining_count == 0,
        "cleanup triggers left {remaining_count} ACL rows after deleting principals"
    );
    Ok(())
}

/// Verify that `grant_resource_permission` rejects an unknown principal ID.
///
/// # Errors
///
/// Returns an error if the row was accepted or a database operation failed.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(super) async fn reject_unknown_principal_body(conn: &mut DbConnection) -> Result<(), AnyError> {
    let owner_id = create_test_user(conn, "principal-owner").await?;
    let permission_id = seed_download_permission(conn).await?;
    let node_id = create_root_file_node_for_owner(
        conn,
        owner_id,
        RootFileNodeSpec {
            name: "principal-check.txt",
            object_key: "objects/principal-check.txt",
            size: 5,
        },
    )
    .await?;

    let Err(err) = grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "user",
            principal_id: i32::MAX,
            permission_id,
        },
    )
    .await
    else {
        anyhow::bail!("unknown principal should be rejected");
    };
    anyhow::ensure!(
        err.to_string().contains("resource_permissions principal"),
        "unknown principal returned unexpected error: {err}"
    );
    Ok(())
}
