use diesel::{dsl::sql, sql_types::Integer, sqlite::SqliteConnection};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts, columns, table_columns};

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
