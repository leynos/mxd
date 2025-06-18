use diesel::{Connection, dsl::sql, sql_types::Integer};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

fn sqlite_sync() -> Vec<i32> {
    use diesel::{RunQueryDsl, sqlite::SqliteConnection};
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    SqliteConnection::with_recursive(
        "t",
        &["n"],
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
            sql::<Integer>("SELECT n FROM t"),
        ),
    )
    .load(&mut conn)
    .unwrap()
}

async fn sqlite_async() -> Vec<i32> {
    use diesel::sqlite::SqliteConnection;
    use diesel_async::{
        AsyncConnection,
        RunQueryDsl,
        sync_connection_wrapper::SyncConnectionWrapper,
    };
    let mut conn = SyncConnectionWrapper::<SqliteConnection>::establish(":memory:")
        .await
        .unwrap();
    SyncConnectionWrapper::<SqliteConnection>::with_recursive(
        "t",
        &["n"],
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
            sql::<Integer>("SELECT n FROM t"),
        ),
    )
    .load(&mut conn)
    .await
    .unwrap()
}

async fn pg_async() -> Vec<i32> {
    use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
    use postgresql_embedded::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().await.unwrap();
    pg.start().await.unwrap();
    pg.create_database("test").await.unwrap();
    let url = pg.settings().url("test");
    let res = {
        let mut conn = AsyncPgConnection::establish(&url).await.unwrap();
        AsyncPgConnection::with_recursive(
            "t",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                sql::<Integer>("SELECT n FROM t"),
            ),
        )
        .load(&mut conn)
        .await
        .unwrap()
    };
    pg.stop().await.unwrap();
    res
}

fn pg_sync() -> Vec<i32> {
    use diesel::{RunQueryDsl, pg::PgConnection};
    use postgresql_embedded::blocking::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().unwrap();
    pg.start().unwrap();
    pg.create_database("test").unwrap();
    let url = pg.settings().url("test");
    let res = {
        let mut conn = PgConnection::establish(&url).unwrap();
        PgConnection::with_recursive(
            "t",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                sql::<Integer>("SELECT n FROM t"),
            ),
        )
        .load(&mut conn)
        .unwrap()
    };
    pg.stop().unwrap();
    res
}

#[test]
fn test_sqlite_sync() {
    assert_eq!(sqlite_sync(), vec![1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn test_sqlite_async() {
    assert_eq!(sqlite_async().await, vec![1, 2, 3, 4, 5]);
}

#[tokio::test]
#[ignore]
async fn test_pg_async() {
    assert_eq!(pg_async().await, vec![1, 2, 3, 4, 5]);
}

#[test]
#[ignore]
fn test_pg_sync() {
    assert_eq!(pg_sync(), vec![1, 2, 3, 4, 5]);
}
