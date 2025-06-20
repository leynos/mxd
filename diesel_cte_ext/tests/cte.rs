//! Behavioural tests for recursive CTE helpers using SQLite and PostgreSQL.

use diesel::{dsl::sql, sql_types::Integer};
use diesel_cte_ext::{Columns, RecursiveCTEExt, RecursiveParts};
mod pg_util;

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
    use diesel_async::{AsyncPgConnection, RunQueryDsl};

    pg_util::with_pg_async(|conn| async move {
        AsyncPgConnection::with_recursive(
            "t",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                sql::<Integer>("SELECT n FROM t"),
            ),
        )
        .load(conn)
        .await
        .unwrap()
    })
    .await
}

fn pg_sync() -> Vec<i32> {
    use diesel::{RunQueryDsl, pg::PgConnection};

    pg_util::with_pg_sync(|conn| {
        PgConnection::with_recursive(
            "t",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                sql::<Integer>("SELECT n FROM t"),
            ),
        )
        .load(conn)
        .unwrap()
    })
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
