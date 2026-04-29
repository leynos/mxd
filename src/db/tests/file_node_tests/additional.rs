//! Postgres entry points for additional shared file-node scenarios.

#[cfg(feature = "postgres")]
macro_rules! run_pg_scenario {
    ($db_name:literal, $body:path, $expectation:literal) => {
        super::with_embedded_pg($db_name, |conn| Box::pin($body(conn)))
            .await
            .expect($expectation)
    };
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resolve_file_node_path_returns_none_for_missing_path() {
    run_pg_scenario!(
        "missing_path",
        super::resolve_file_node_path_returns_none_for_missing_path_body,
        "missing-path should return None"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_non_download_permission_does_not_grant_visibility() {
    run_pg_scenario!(
        "non_download_permission",
        super::non_download_permission_does_not_grant_visibility_body,
        "non-download permission should not grant visibility"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_nested_child_not_visible_without_explicit_grant() {
    run_pg_scenario!(
        "nested_child_visibility",
        super::nested_child_not_visible_without_explicit_grant_body,
        "nested child should not appear without explicit grant"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_node_check_kind_constraints() {
    run_pg_scenario!(
        "kind_constraints",
        super::file_node_check_kind_constraint_body,
        "kind-specific CHECK constraints should be enforced"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_grant_revocation_removes_visibility() {
    run_pg_scenario!(
        "grant_revocation_visibility",
        super::grant_revocation_removes_visibility_body,
        "revoked grant should remove visibility"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_group_membership_removal_revokes_visibility() {
    run_pg_scenario!(
        "group_membership_removal",
        super::group_membership_removal_revokes_visibility_body,
        "group membership removal should revoke visibility"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_visible_root_files_merge_postgres() {
    run_pg_scenario!(
        "visible_root_merge",
        super::visible_root_files_merge_body,
        "postgres: legacy + modern visibility results should be merged and ordered"
    );
}
