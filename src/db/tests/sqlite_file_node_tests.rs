//! `SQLite` wrappers for shared file-node database scenarios.
//!
//! These tests provide migrated in-memory `SQLite` connections and delegate the
//! actual behaviour assertions to backend-agnostic scenario bodies.

use super::*;

#[rstest]
#[tokio::test]
async fn test_file_node_acl_flow(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::file_node_acl_flow_body(&mut conn).await
}

#[rstest]
#[tokio::test]
async fn test_resolve_file_node_path_and_alias(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::resolve_file_node_path_and_alias_body(&mut conn).await
}

#[rstest]
#[tokio::test]
async fn test_group_acl_visibility(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::group_acl_visibility_body(&mut conn).await
}

#[rstest]
#[tokio::test]
async fn test_grant_revocation_removes_visibility(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    file_node_tests::grant_revocation_removes_visibility_body(&mut conn)
        .await
        .expect("revoked grant should remove visibility");
}

#[rstest]
#[tokio::test]
async fn test_group_membership_removal_revokes_visibility(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    file_node_tests::group_membership_removal_revokes_visibility_body(&mut conn)
        .await
        .expect("group membership removal should revoke visibility");
}

#[rstest]
#[tokio::test]
async fn test_resolve_file_node_path_returns_none_for_missing_path(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    file_node_tests::resolve_file_node_path_returns_none_for_missing_path_body(&mut conn)
        .await
        .expect("missing-path should return None");
}

#[rstest]
#[tokio::test]
async fn test_non_download_permission_does_not_grant_visibility(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    file_node_tests::non_download_permission_does_not_grant_visibility_body(&mut conn)
        .await
        .expect("non-download permission should not grant visibility");
}

#[rstest]
#[tokio::test]
async fn test_nested_child_not_visible_without_explicit_grant(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    file_node_tests::nested_child_not_visible_without_explicit_grant_body(&mut conn)
        .await
        .expect("nested child should not appear without explicit grant");
}

#[rstest]
#[tokio::test]
async fn test_file_node_check_kind_constraints(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    file_node_tests::file_node_check_kind_constraint_body(&mut conn)
        .await
        .expect("kind-specific CHECK constraints should be enforced");
}

#[rstest]
#[tokio::test]
async fn test_file_nodes_reject_self_parent(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::reject_self_parent_body(&mut conn, "CHECK constraint failed")
        .await
        .expect("self-parent guard should reject recursive parent links");
}

#[rstest]
#[tokio::test]
async fn test_file_nodes_reject_invalid_basenames(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::reject_invalid_basenames_body(&mut conn, "CHECK constraint failed")
        .await
        .expect("basename guard should reject empty and slash-delimited names");
}

#[rstest]
#[tokio::test]
async fn test_resource_permissions_cleanup_on_principal_delete(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::cleanup_on_principal_delete_body(&mut conn)
        .await
        .expect("principal deletes should clean up ACL rows");
}

#[rstest]
#[tokio::test]
async fn test_resource_permissions_reject_unknown_principal(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::reject_unknown_principal_body(&mut conn)
        .await
        .expect("unknown principals should be rejected");
}
