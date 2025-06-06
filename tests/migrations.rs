use diesel_async::{AsyncConnection, RunQueryDsl};
use mxd::db;

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_migrations_run() {
    let mut conn = db::DbConnection::establish(":memory:").await.unwrap();
    db::run_migrations(&mut conn).await.unwrap();
    diesel::sql_query("SELECT * FROM users")
        .execute(&mut conn)
        .await
        .unwrap();
    diesel::sql_query("SELECT * FROM news_articles")
        .execute(&mut conn)
        .await
        .unwrap();
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_migrations_run() {
    use pg_embed::fetch::{PG_V15, PgFetchSettings};
    use pg_embed::postgres::{PgAuthMethod, PgEmbed, PgSettings};
    use std::net::TcpListener;
    use std::time::Duration;
    use tempfile::TempDir;
    let temp = TempDir::new().unwrap();
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    let pg_settings = PgSettings {
        database_dir: temp.path().to_path_buf(),
        port,
        user: "postgres".to_string(),
        password: "password".to_string(),
        auth_method: PgAuthMethod::Plain,
        persistent: false,
        timeout: Some(Duration::from_secs(15)),
        migration_dir: None,
    };
    let fetch_settings = PgFetchSettings {
        version: PG_V15,
        ..Default::default()
    };
    let mut pg = PgEmbed::new(pg_settings, fetch_settings).await.unwrap();
    pg.setup().await.unwrap();
    pg.start_db().await.unwrap();
    let mut conn = db::DbConnection::establish(&pg.db_uri).await.unwrap();
    db::run_migrations(&mut conn).await.unwrap();
    diesel::sql_query("SELECT * FROM users")
        .execute(&mut conn)
        .await
        .unwrap();
    diesel::sql_query("SELECT * FROM news_articles")
        .execute(&mut conn)
        .await
        .unwrap();
    pg.stop_db().await.unwrap();
}
