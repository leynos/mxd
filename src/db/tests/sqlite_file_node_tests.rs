//! `SQLite` wrappers for shared file-node database scenarios.
//!
//! These tests provide migrated in-memory `SQLite` connections and delegate the
//! actual behaviour assertions to backend-agnostic scenario bodies.

use std::{future::Future, pin::Pin};

use super::*;

type ScenarioFuture<'conn> = Pin<Box<dyn Future<Output = Result<(), AnyError>> + 'conn>>;
type ScenarioBody = for<'conn> fn(&'conn mut DbConnection) -> ScenarioFuture<'conn>;

macro_rules! scenario_body {
    ($adapter:ident, $body:path) => {
        fn $adapter(conn: &mut DbConnection) -> ScenarioFuture<'_> { Box::pin($body(conn)) }
    };
}

scenario_body!(file_node_acl_flow, file_node_tests::file_node_acl_flow_body);
scenario_body!(
    resolve_file_node_path_and_alias,
    file_node_tests::resolve_file_node_path_and_alias_body
);
scenario_body!(
    group_acl_visibility,
    file_node_tests::group_acl_visibility_body
);
scenario_body!(
    grant_revocation_removes_visibility,
    file_node_tests::grant_revocation_removes_visibility_body
);
scenario_body!(
    group_membership_removal_revokes_visibility,
    file_node_tests::group_membership_removal_revokes_visibility_body
);
scenario_body!(
    resolve_file_node_path_returns_none_for_missing_path,
    file_node_tests::resolve_file_node_path_returns_none_for_missing_path_body
);
scenario_body!(
    non_download_permission_does_not_grant_visibility,
    file_node_tests::non_download_permission_does_not_grant_visibility_body
);
scenario_body!(
    nested_child_not_visible_without_explicit_grant,
    file_node_tests::nested_child_not_visible_without_explicit_grant_body
);
scenario_body!(
    file_node_check_kind_constraints,
    file_node_tests::file_node_check_kind_constraint_body
);
scenario_body!(
    resource_permissions_cleanup_on_principal_delete,
    file_node_tests::cleanup_on_principal_delete_body
);
scenario_body!(
    resource_permissions_reject_unknown_principal,
    file_node_tests::reject_unknown_principal_body
);

#[rstest]
#[case(file_node_acl_flow)]
#[case(resolve_file_node_path_and_alias)]
#[case(group_acl_visibility)]
#[case(grant_revocation_removes_visibility)]
#[case(group_membership_removal_revokes_visibility)]
#[case(resolve_file_node_path_returns_none_for_missing_path)]
#[case(non_download_permission_does_not_grant_visibility)]
#[case(nested_child_not_visible_without_explicit_grant)]
#[case(file_node_check_kind_constraints)]
#[case(resource_permissions_cleanup_on_principal_delete)]
#[case(resource_permissions_reject_unknown_principal)]
#[tokio::test]
async fn test_file_node_shared_scenario(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
    #[case] body: ScenarioBody,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn.await?;
    body(&mut conn).await
}

#[rstest]
#[tokio::test]
async fn test_file_nodes_reject_self_parent(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn.await?;
    file_node_tests::reject_self_parent_body(&mut conn, "CHECK constraint failed").await?;
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_file_nodes_reject_invalid_basenames(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn.await?;
    file_node_tests::reject_invalid_basenames_body(&mut conn, "CHECK constraint failed").await?;
    Ok(())
}
