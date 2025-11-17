//! Helpers for retrieving `SQLite` row ids when `RETURNING` is unavailable.

#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
use diesel::result::QueryResult;
#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
use diesel_async::RunQueryDsl;

#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
use super::connection::DbConnection;

/// Fetch the last inserted row id on SQLite.
#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
pub async fn fetch_last_insert_rowid(conn: &mut DbConnection) -> QueryResult<i32> {
    use diesel::sql_types::Integer;
    diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()"))
        .get_result(conn)
        .await
}
