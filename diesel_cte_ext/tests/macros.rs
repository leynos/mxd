//! Compile-time tests verifying the columns macros build valid queries.
use diesel::{dsl::sql, sql_types::Integer, sqlite::SqliteConnection};
use diesel_cte_ext::{
    RecursiveCTEExt,
    RecursiveParts,
    columns,
    seed_query,
    step_query,
    table_columns,
};

#[test]
fn table_columns_compile() {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    let _ = SqliteConnection::with_recursive(
        "t",
        table_columns!(crate::schema::users::table),
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT 1"),
        ),
    )
    .load::<i32>(&mut conn);
}

#[test]
fn columns_macro_compile() {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    let _ = SqliteConnection::with_recursive(
        "t",
        columns!(crate::schema::users::id, crate::schema::users::username),
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT 1"),
        ),
    )
    .load::<i32>(&mut conn);
}

#[test]
fn query_macros_sqlite() {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    let rows: Vec<i32> = SqliteConnection::with_recursive(
        "t",
        &["n"],
        RecursiveParts::new(
            seed_query!(sql::<Integer>("SELECT 1")),
            step_query!(sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5")),
            step_query!(sql::<Integer>("SELECT n FROM t")),
        ),
    )
    .load(&mut conn)
    .unwrap();
    assert_eq!(rows, vec![1, 2, 3, 4, 5]);
}

// TODO: re-enable once embedded Postgres works in CI
#[tokio::test]
#[ignore]
async fn query_macros_postgres() {
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
                seed_query!(sql::<Integer>("SELECT 1")),
                step_query!(sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5")),
                step_query!(sql::<Integer>("SELECT n FROM t")),
            ),
        )
        .load(&mut conn)
        .await
        .unwrap()
    };
    pg.stop().await.unwrap();
    assert_eq!(res, vec![1, 2, 3, 4, 5]);
}
