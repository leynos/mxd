//! Helpers for retrieving `SQLite` row ids when `RETURNING` is unavailable.

#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
mod inner {
    //! SQLite-specific helpers for retrieving last insert row ids.

    use diesel::{result::QueryResult, sql_types::Integer};
    use diesel_async::RunQueryDsl;

    use super::super::connection::DbConnection;

    /// Fetch the last inserted row id on `SQLite`.
    ///
    /// # Important
    /// Call this immediately after the corresponding `INSERT` using the same
    /// connection. Executing any other statement in between can change the
    /// value returned by `last_insert_rowid()`.
    ///
    /// # Errors
    /// Returns any error produced by Diesel while querying `last_insert_rowid()`.
    pub async fn fetch_last_insert_rowid(conn: &mut DbConnection) -> QueryResult<i32> {
        diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()"))
            .get_result(conn)
            .await
    }
}

#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
pub use inner::fetch_last_insert_rowid;
