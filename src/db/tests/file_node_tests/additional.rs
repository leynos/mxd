//! Postgres entry points for additional shared file-node scenarios.

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resolve_file_node_path_returns_none_for_missing_path() {
    super::with_embedded_pg("missing_path", |conn| {
        Box::pin(super::resolve_file_node_path_returns_none_for_missing_path_body(conn))
    })
    .await
    .expect("missing-path should return None");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_non_download_permission_does_not_grant_visibility() {
    super::with_embedded_pg("non_download_permission", |conn| {
        Box::pin(super::non_download_permission_does_not_grant_visibility_body(conn))
    })
    .await
    .expect("non-download permission should not grant visibility");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_nested_child_not_visible_without_explicit_grant() {
    super::with_embedded_pg("nested_child_visibility", |conn| {
        Box::pin(super::nested_child_not_visible_without_explicit_grant_body(
            conn,
        ))
    })
    .await
    .expect("nested child should not appear without explicit grant");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_node_check_kind_constraints() {
    super::with_embedded_pg("kind_constraints", |conn| {
        Box::pin(super::file_node_check_kind_constraint_body(conn))
    })
    .await
    .expect("kind-specific CHECK constraints should be enforced");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_visible_root_files_merge_postgres() {
    super::with_embedded_pg("visible_root_merge", |conn| {
        Box::pin(async move { super::visible_root_files_merge_body(conn).await })
    })
    .await
    .expect("postgres: legacy + modern visibility results should be merged and ordered");
}
