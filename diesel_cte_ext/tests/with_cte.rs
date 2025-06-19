use diesel::{Connection, dsl::sql, sql_types::Integer};
use diesel_cte_ext::RecursiveCTEExt;

fn sqlite_sync() -> i32 {
    use diesel::{RunQueryDsl, sqlite::SqliteConnection};
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    SqliteConnection::with_cte(
        "t",
        &["n"],
        sql::<Integer>("SELECT 42"),
        sql::<Integer>("SELECT n FROM t"),
    )
    .get_result(&mut conn)
    .unwrap()
}

async fn sqlite_async() -> i32 {
    use diesel::sqlite::SqliteConnection;
    use diesel_async::{
        AsyncConnection,
        RunQueryDsl,
        sync_connection_wrapper::SyncConnectionWrapper,
    };
    let mut conn = SyncConnectionWrapper::<SqliteConnection>::establish(":memory:")
        .await
        .unwrap();
    SyncConnectionWrapper::<SqliteConnection>::with_cte(
        "t",
        &["n"],
        sql::<Integer>("SELECT 42"),
        sql::<Integer>("SELECT n FROM t"),
    )
    .get_result(&mut conn)
    .await
    .unwrap()
}

async fn pg_async() -> i32 {
    use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
    use postgresql_embedded::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().await.unwrap();
    pg.start().await.unwrap();
    pg.create_database("test").await.unwrap();
    let url = pg.settings().url("test");
    let res = {
        let mut conn = AsyncPgConnection::establish(&url).await.unwrap();
        AsyncPgConnection::with_cte(
            "t",
            &["n"],
            sql::<Integer>("SELECT 42"),
            sql::<Integer>("SELECT n FROM t"),
        )
        .get_result(&mut conn)
        .await
        .unwrap()
    };
    pg.stop().await.unwrap();
    res
}

fn pg_sync() -> i32 {
    use diesel::{RunQueryDsl, pg::PgConnection};
    use postgresql_embedded::blocking::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().unwrap();
    pg.start().unwrap();
    pg.create_database("test").unwrap();
    let url = pg.settings().url("test");
    let res = {
        let mut conn = PgConnection::establish(&url).unwrap();
        PgConnection::with_cte(
            "t",
            &["n"],
            sql::<Integer>("SELECT 42"),
            sql::<Integer>("SELECT n FROM t"),
        )
        .get_result(&mut conn)
        .unwrap()
    };
    pg.stop().unwrap();
    res
}

#[test]
fn test_sqlite_sync() {
    assert_eq!(sqlite_sync(), 42);
}

#[tokio::test]
async fn test_sqlite_async() {
    assert_eq!(sqlite_async().await, 42);
}

#[tokio::test]
#[ignore]
async fn test_pg_async() {
    assert_eq!(pg_async().await, 42);
}

#[test]
#[ignore]
fn test_pg_sync() {
    assert_eq!(pg_sync(), 42);
}
