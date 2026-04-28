//! SQLite-only legacy file-node test bodies.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use rstest::rstest;
use test_util::AnyError;

use super::super::{audit_sqlite_features, migrated_conn};
use crate::{
    db::{
        DbConnection,
        create_file_node,
        create_user,
        establish_pool,
        get_user_by_name,
        grant_resource_permission,
        list_visible_root_file_nodes_for_user,
        seed_permission,
    },
    models::{FileNodeKind, NewFileNode, NewResourcePermission, NewUser},
    schema::{file_acl::dsl as legacy_file_acl, files::dsl as legacy_files},
};

/// Verify that legacy `files`/`file_acl` rows surface through
/// `list_visible_root_file_nodes_for_user` during the backfill window.
///
/// Inserts a record into the legacy `files` table, confirms no visibility,
/// then inserts a matching `file_acl` row and asserts the file becomes visible
/// with `kind = "file"`.
///
/// # Errors
///
/// Propagates any database error encountered during the scenario.
#[expect(
    clippy::cognitive_complexity,
    reason = "scenario-style test body is clearer kept as a linear flow"
)]
pub(crate) async fn legacy_file_acl_visibility_fallback_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    create_user(
        conn,
        &NewUser {
            username: "frank",
            password: "hash",
        },
    )
    .await?;
    let frank = get_user_by_name(conn, "frank")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user missing"))?;

    diesel::insert_into(legacy_files::files)
        .values((
            legacy_files::name.eq("legacy.txt"),
            legacy_files::object_key.eq("objects/legacy.txt"),
            legacy_files::size.eq(99_i64),
        ))
        .execute(conn)
        .await?;
    let file_id = legacy_files::files
        .filter(legacy_files::name.eq("legacy.txt"))
        .select(legacy_files::id)
        .first::<i32>(conn)
        .await?;
    let visible_before_acl = list_visible_root_file_nodes_for_user(conn, frank.id).await?;
    assert_eq!(visible_before_acl.len(), 0);

    diesel::insert_into(legacy_file_acl::file_acl)
        .values((
            legacy_file_acl::file_id.eq(file_id),
            legacy_file_acl::user_id.eq(frank.id),
        ))
        .execute(conn)
        .await?;

    let visible = list_visible_root_file_nodes_for_user(conn, frank.id).await?;
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "legacy.txt");
    assert_eq!(visible[0].kind, "file");
    Ok(())
}

/// Verify that modern `file_nodes` and legacy `files` rows are merged and
/// deduplicated correctly in `list_visible_root_file_nodes_for_user`.
///
/// Seeds one modern `file_node` with a download permission and one legacy
/// `files`/`file_acl` pair for the same user, then asserts both names appear
/// in the result in order.
///
/// # Errors
///
/// Propagates any database error encountered during the scenario.
#[expect(
    clippy::cognitive_complexity,
    reason = "scenario-style test body is clearer kept as a linear flow"
)]
pub(crate) async fn visible_root_files_merge_body(conn: &mut DbConnection) -> Result<(), AnyError> {
    create_user(
        conn,
        &NewUser {
            username: "merge-user",
            password: "hash",
        },
    )
    .await?;
    let merge_user = get_user_by_name(conn, "merge-user")
        .await?
        .ok_or_else(|| anyhow::anyhow!("merge-user missing"))?;

    let permission_id = seed_permission(conn, &crate::db::download_file_permission()).await?;
    let node_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "modern.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("objects/modern.txt"),
            size: Some(11),
            comment: None,
            is_dropbox: false,
            creator_id: merge_user.id,
        },
    )
    .await?;
    diesel::insert_into(legacy_files::files)
        .values((
            legacy_files::name.eq("legacy.txt"),
            legacy_files::object_key.eq("objects/legacy.txt"),
            legacy_files::size.eq(7_i64),
        ))
        .execute(conn)
        .await?;
    let legacy_id = legacy_files::files
        .filter(legacy_files::name.eq("legacy.txt"))
        .select(legacy_files::id)
        .first::<i32>(conn)
        .await?;
    let visible_before_grants = list_visible_root_file_nodes_for_user(conn, merge_user.id).await?;
    assert_eq!(visible_before_grants.len(), 0);

    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "user",
            principal_id: merge_user.id,
            permission_id,
        },
    )
    .await?;
    diesel::insert_into(legacy_file_acl::file_acl)
        .values((
            legacy_file_acl::file_id.eq(legacy_id),
            legacy_file_acl::user_id.eq(merge_user.id),
        ))
        .execute(conn)
        .await?;

    let visible = list_visible_root_file_nodes_for_user(conn, merge_user.id).await?;
    let visible_names = visible
        .into_iter()
        .map(|node| node.name)
        .collect::<Vec<_>>();
    assert_eq!(visible_names, vec!["legacy.txt", "modern.txt"]);
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_legacy_file_acl_visibility_fallback(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    legacy_file_acl_visibility_fallback_body(&mut conn).await
}

#[rstest]
#[tokio::test]
async fn test_visible_root_files_merge_legacy_and_file_node_sources(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    visible_root_files_merge_body(&mut conn).await
}

#[tokio::test]
async fn test_establish_pool_returns_pool() {
    let pool = establish_pool(":memory:")
        .await
        .expect("pool creation failed");
    pool.get().await.expect("pool should yield connection");
}

#[rstest]
#[tokio::test]
async fn test_audit_features(#[future] migrated_conn: Result<DbConnection, AnyError>) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    audit_sqlite_features(&mut conn)
        .await
        .expect("sqlite feature audit failed");
}
