//! `Postgres` wrappers for shared file-node database scenarios.
//!
//! These tests run backend-agnostic scenario bodies through the shared
//! embedded-`Postgres` harness so the same file-node behaviours are covered
//! under the `Postgres` feature set.

use super::file_node_tests;
use crate::db::audit_postgres_features;

#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_node_acl_flow() {
    file_node_tests::with_embedded_pg("file_node_acl_flow", |conn| {
        Box::pin(file_node_tests::file_node_acl_flow_body(conn))
    })
    .await
    .expect("file-node ACL flow should pass on Postgres");
}

#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resolve_file_node_path_and_alias() {
    file_node_tests::with_embedded_pg("resolve_file_node_path_alias", |conn| {
        Box::pin(file_node_tests::resolve_file_node_path_and_alias_body(conn))
    })
    .await
    .expect("file-node path and alias resolution should pass on Postgres");
}

#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_group_acl_visibility() {
    file_node_tests::with_embedded_pg("group_acl_visibility", |conn| {
        Box::pin(file_node_tests::group_acl_visibility_body(conn))
    })
    .await
    .expect("group ACL visibility should pass on Postgres");
}

#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_nodes_reject_self_parent() {
    file_node_tests::with_embedded_pg("self_parent", |conn| {
        Box::pin(file_node_tests::reject_self_parent_body(conn, "check"))
    })
    .await
    .expect("self-parent guard should reject recursive parent links");
}

#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_nodes_reject_invalid_basenames() {
    file_node_tests::with_embedded_pg("invalid_basenames", |conn| {
        Box::pin(file_node_tests::reject_invalid_basenames_body(
            conn, "check",
        ))
    })
    .await
    .expect("basename guard should reject empty and slash-delimited names");
}

#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resource_permissions_cleanup_on_principal_delete() {
    file_node_tests::with_embedded_pg("cleanup_principal_delete", |conn| {
        Box::pin(file_node_tests::cleanup_on_principal_delete_body(conn))
    })
    .await
    .expect("principal deletes should clean up ACL rows");
}

#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resource_permissions_reject_unknown_principal() {
    file_node_tests::with_embedded_pg("unknown_principal", |conn| {
        Box::pin(file_node_tests::reject_unknown_principal_body(conn))
    })
    .await
    .expect("unknown principals should be rejected");
}

#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_audit_postgres() {
    file_node_tests::with_embedded_pg("audit_postgres", |conn| {
        Box::pin(async move {
            audit_postgres_features(conn)
                .await
                .expect("postgres feature audit failed");
            Ok(())
        })
    })
    .await
    .expect("postgres feature audit should run");
}
