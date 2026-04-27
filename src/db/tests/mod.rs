#[cfg(any(feature = "sqlite", feature = "postgres"))]
use diesel_async::AsyncConnection;
#[cfg(feature = "sqlite")]
use rstest::{fixture, rstest};
#[cfg(feature = "sqlite")]
use test_util::AnyError;

#[cfg(any(feature = "sqlite", feature = "postgres"))]
mod file_node_tests;

use super::*;
#[cfg(feature = "sqlite")]
use crate::models::{NewBundle, NewCategory, NewUser};

#[cfg(feature = "sqlite")]
#[fixture]
async fn migrated_conn() -> Result<DbConnection, AnyError> {
    let mut conn = DbConnection::establish(":memory:").await?;
    apply_migrations(&mut conn, "", None).await?;
    Ok(conn)
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_and_get_user(#[future] migrated_conn: Result<DbConnection, AnyError>) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    let new_user = NewUser {
        username: "alice",
        password: "hash",
    };
    create_user(&mut conn, &new_user)
        .await
        .expect("failed to create user");
    let fetched = get_user_by_name(&mut conn, "alice")
        .await
        .expect("lookup failed")
        .expect("user not found");
    assert_eq!(fetched.username, "alice");
    assert_eq!(fetched.password, "hash");
}

// basic smoke test for migrations and insertion
#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_bundle_and_category(#[future] migrated_conn: Result<DbConnection, AnyError>) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    let bun = NewBundle {
        parent_bundle_id: None,
        name: "Bundle",
    };
    let _ = create_bundle(&mut conn, &bun)
        .await
        .expect("failed to create bundle");
    let cat = NewCategory {
        name: "General",
        bundle_id: None,
    };
    create_category(&mut conn, &cat)
        .await
        .expect("failed to create category");
    let _names = list_names_at_path(&mut conn, None)
        .await
        .expect("failed to list names");
}

#[cfg(feature = "sqlite")]
async fn seed_root_category(conn: &mut DbConnection, name: &'static str) -> Result<(), AnyError> {
    let cat = NewCategory {
        name,
        bundle_id: None,
    };
    create_category(conn, &cat).await?;
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_list_names_invalid_path(#[future] migrated_conn: Result<DbConnection, AnyError>) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    // Ensure we have at least one bundle to differentiate root vs invalid lookups.
    let bun = NewBundle {
        parent_bundle_id: None,
        name: "RootBundle",
    };
    create_bundle(&mut conn, &bun)
        .await
        .expect("failed to create bundle");
    let err = list_names_at_path(&mut conn, Some("/missing"))
        .await
        .expect_err("expected invalid path error");
    assert!(matches!(err, PathLookupError::InvalidPath));
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_root_article_round_trip(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    seed_root_category(&mut conn, "General")
        .await
        .expect("failed to seed category");
    let title = "Root Article".to_string();
    let data = "Hello, world!".to_string();
    let params = CreateRootArticleParams {
        title: &title,
        flags: 0,
        data_flavor: "text/plain",
        data: &data,
    };
    let article_id = create_root_article(&mut conn, "/General", params)
        .await
        .expect("failed to create article");
    let fetched = get_article(&mut conn, "/General", article_id)
        .await
        .expect("lookup failed")
        .expect("article missing");
    assert_eq!(fetched.id, article_id);
    assert_eq!(fetched.title, title);
    let titles = list_article_titles(&mut conn, "/General")
        .await
        .expect("failed to list titles");
    assert_eq!(titles, vec![title]);
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_root_article_invalid_path(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    let params = CreateRootArticleParams {
        title: "Ghost",
        flags: 0,
        data_flavor: "text/plain",
        data: "ghost",
    };
    let err = create_root_article(&mut conn, "/missing", params)
        .await
        .expect_err("expected invalid path failure");
    assert!(matches!(err, PathLookupError::InvalidPath));
}

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_nodes_reject_self_parent() {
    file_node_tests::with_embedded_pg("self_parent", |conn| {
        Box::pin(file_node_tests::reject_self_parent_body(conn, "check"))
    })
    .await
    .expect("self-parent guard should reject recursive parent links");
}

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "postgres")]
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

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resource_permissions_cleanup_on_principal_delete() {
    file_node_tests::with_embedded_pg("cleanup_principal_delete", |conn| {
        Box::pin(file_node_tests::cleanup_on_principal_delete_body(conn))
    })
    .await
    .expect("principal deletes should clean up ACL rows");
}

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resource_permissions_reject_unknown_principal() {
    file_node_tests::with_embedded_pg("unknown_principal", |conn| {
        Box::pin(file_node_tests::reject_unknown_principal_body(conn))
    })
    .await
    .expect("unknown principals should be rejected");
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_legacy_file_acl_visibility_fallback(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::legacy_file_acl_visibility_fallback_body(&mut conn).await
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_visible_root_files_merge_legacy_and_file_node_sources(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    file_node_tests::visible_root_files_merge_body(&mut conn).await
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_establish_pool_returns_pool() {
    let pool = establish_pool(":memory:")
        .await
        .expect("pool creation failed");
    pool.get().await.expect("pool should yield connection");
}

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires embedded PostgreSQL server"]
async fn test_audit_postgres() {
    use postgresql_embedded::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().await.expect("failed to set up postgres");
    pg.start().await.expect("failed to start postgres");
    pg.create_database("test")
        .await
        .expect("failed to create db");
    let url = pg.settings().url("test");
    let mut conn = diesel_async::AsyncPgConnection::establish(&url)
        .await
        .expect("failed to connect to postgres");
    audit_postgres_features(&mut conn)
        .await
        .expect("postgres feature audit failed");
    pg.stop().await.expect("failed to stop postgres");
}
