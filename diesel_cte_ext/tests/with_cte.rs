use diesel::{Connection, dsl::sql, sql_types::Integer};
use diesel_cte_ext::RecursiveCTEExt;

fn with_pg_sync<F, R>(f: F) -> R
where
    F: FnOnce(&mut diesel::pg::PgConnection) -> R,
{
    use diesel::pg::PgConnection;
    use postgresql_embedded::blocking::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().unwrap();
    pg.start().unwrap();
    pg.create_database("test").unwrap();
    let url = pg.settings().url("test");
    let res = {
        let mut conn = PgConnection::establish(&url).unwrap();
        f(&mut conn)
    };
    pg.stop().unwrap();
    res
}

async fn with_pg_async<F, Fut, R>(f: F) -> R
where
    F: FnOnce(&mut diesel_async::AsyncPgConnection) -> Fut,
    Fut: std::future::Future<Output = R>,
{
    use diesel_async::{AsyncConnection, AsyncPgConnection};
    use postgresql_embedded::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().await.unwrap();
    pg.start().await.unwrap();
    pg.create_database("test").await.unwrap();
    let url = pg.settings().url("test");
    let res = {
        let mut conn = AsyncPgConnection::establish(&url).await.unwrap();
        f(&mut conn).await
    };
    pg.stop().await.unwrap();
    res
}

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
    use diesel_async::RunQueryDsl;

    with_pg_async(|conn| async move {
        diesel_async::AsyncPgConnection::with_cte(
            "t",
            &["n"],
            sql::<Integer>("SELECT 42"),
            sql::<Integer>("SELECT n FROM t"),
        )
        .get_result(conn)
        .await
        .unwrap()
    })
    .await
}

fn pg_sync() -> i32 {
    use diesel::RunQueryDsl;

    with_pg_sync(|conn| {
        diesel::pg::PgConnection::with_cte(
            "t",
            &["n"],
            sql::<Integer>("SELECT 42"),
            sql::<Integer>("SELECT n FROM t"),
        )
        .get_result(conn)
        .unwrap()
    })
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
