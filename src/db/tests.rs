#[cfg(feature = "sqlite")]
use diesel::prelude::*;
use diesel_async::AsyncConnection;
#[cfg(feature = "sqlite")]
use diesel_async::RunQueryDsl;
#[cfg(feature = "sqlite")]
use rstest::{fixture, rstest};

use super::*;
#[cfg(feature = "sqlite")]
use crate::{
    models::{NewBundle, NewCategory, NewFileAcl, NewFileEntry, NewUser},
    schema::files::dsl as files,
};

#[cfg(feature = "sqlite")]
#[fixture]
async fn migrated_conn() -> DbConnection {
    let mut conn = DbConnection::establish(":memory:")
        .await
        .expect("failed to create in-memory connection");
    apply_migrations(&mut conn, "")
        .await
        .expect("failed to apply migrations");
    conn
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_and_get_user(#[future] migrated_conn: DbConnection) {
    let mut conn = migrated_conn.await;
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
async fn test_create_bundle_and_category(#[future] migrated_conn: DbConnection) {
    let mut conn = migrated_conn.await;
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
async fn seed_root_category(conn: &mut DbConnection, name: &'static str) {
    let cat = NewCategory {
        name,
        bundle_id: None,
    };
    create_category(conn, &cat)
        .await
        .expect("failed to seed category");
}

#[cfg(feature = "sqlite")]
async fn fetch_file_id(conn: &mut DbConnection, name: &str) -> i32 {
    files::files
        .filter(files::name.eq(name))
        .select(files::id)
        .first::<i32>(conn)
        .await
        .expect("file id")
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_list_names_invalid_path(#[future] migrated_conn: DbConnection) {
    let mut conn = migrated_conn.await;
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
async fn test_create_root_article_round_trip(#[future] migrated_conn: DbConnection) {
    let mut conn = migrated_conn.await;
    seed_root_category(&mut conn, "General").await;
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
async fn test_create_root_article_invalid_path(#[future] migrated_conn: DbConnection) {
    let mut conn = migrated_conn.await;
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
async fn test_file_acl_flow(#[future] migrated_conn: DbConnection) {
    let mut conn = migrated_conn.await;
    let file = NewFileEntry {
        name: "report.txt",
        object_key: "objects/report.txt",
        size: 42,
    };
    create_file(&mut conn, &file)
        .await
        .expect("failed to create file");
    let file_id = fetch_file_id(&mut conn, "report.txt").await;

    let user = NewUser {
        username: "carol",
        password: "hash",
    };
    create_user(&mut conn, &user)
        .await
        .expect("failed to create user");
    let carol = get_user_by_name(&mut conn, "carol")
        .await
        .expect("lookup failed")
        .expect("user missing");

    let acl = NewFileAcl {
        file_id,
        user_id: carol.id,
    };
    assert!(
        add_file_acl(&mut conn, &acl)
            .await
            .expect("failed to add acl")
    );
    // second insert is a no-op
    assert!(
        !add_file_acl(&mut conn, &acl)
            .await
            .expect("idempotent acl add")
    );

    let files = list_files_for_user(&mut conn, carol.id)
        .await
        .expect("failed to list files");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "report.txt");
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
async fn test_audit_features(#[future] migrated_conn: DbConnection) {
    let mut conn = migrated_conn.await;
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
