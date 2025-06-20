use diesel::{dsl::sql, sql_types::Integer};
use diesel_cte_ext::RecursiveCTEExt;

const SELECT_42: &str = "SELECT 42";
const SELECT_N_FROM_T: &str = "SELECT n FROM t";
mod pg_util;

fn sqlite_sync() -> i32 {
    use diesel::{RunQueryDsl, sqlite::SqliteConnection};
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    SqliteConnection::with_cte(
        "t",
        &["n"],
        sql::<Integer>(SELECT_42),
        sql::<Integer>(SELECT_N_FROM_T),
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
        sql::<Integer>(SELECT_42),
        sql::<Integer>(SELECT_N_FROM_T),
    )
    .get_result(&mut conn)
    .await
    .unwrap()
}

async fn pg_async() -> i32 {
    use diesel_async::RunQueryDsl;

    pg_util::with_pg_async(|conn| async move {
        diesel_async::AsyncPgConnection::with_cte(
            "t",
            &["n"],
            sql::<Integer>(SELECT_42),
            sql::<Integer>(SELECT_N_FROM_T),
        )
        .get_result(conn)
        .await
        .unwrap()
    })
    .await
}

fn pg_sync() -> i32 {
    use diesel::RunQueryDsl;

    pg_util::with_pg_sync(|conn| {
        diesel::pg::PgConnection::with_cte(
            "t",
            &["n"],
            sql::<Integer>(SELECT_42),
            sql::<Integer>(SELECT_N_FROM_T),
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
