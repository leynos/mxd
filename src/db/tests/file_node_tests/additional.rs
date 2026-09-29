//! Postgres entry points for additional shared file-node scenarios.

#[cfg(feature = "postgres")]
macro_rules! run_pg_scenario {
    ($body:path, $expectation:literal) => {
        super::with_embedded_pg(|conn| Box::pin($body(conn)))
            .await
            .expect($expectation)
    };
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resolve_file_node_path_returns_none_for_missing_path() {
    run_pg_scenario!(
        super::resolve_file_node_path_returns_none_for_missing_path_body,
        "missing-path should return None"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_non_download_permission_does_not_grant_visibility() {
    run_pg_scenario!(
        super::non_download_permission_does_not_grant_visibility_body,
        "non-download permission should not grant visibility"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_nested_child_not_visible_without_explicit_grant() {
    run_pg_scenario!(
        super::nested_child_not_visible_without_explicit_grant_body,
        "nested child should not appear without explicit grant"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_node_check_kind_constraints() {
    run_pg_scenario!(
        super::file_node_check_kind_constraint_body,
        "kind-specific CHECK constraints should be enforced"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_grant_revocation_removes_visibility() {
    run_pg_scenario!(
        super::grant_revocation_removes_visibility_body,
        "revoked grant should remove visibility"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_group_membership_removal_revokes_visibility() {
    run_pg_scenario!(
        super::group_membership_removal_revokes_visibility_body,
        "group membership removal should revoke visibility"
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_visible_root_files_merge_postgres() {
    run_pg_scenario!(
        super::visible_root_files_merge_body,
        "postgres: legacy + modern visibility results should be merged and ordered"
    );
}
