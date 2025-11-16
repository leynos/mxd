use diesel_async::AsyncConnection;
#[cfg(feature = "sqlite")]
use rstest::{fixture, rstest};

use super::*;
#[cfg(feature = "sqlite")]
use crate::models::{NewBundle, NewCategory, NewUser};

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
