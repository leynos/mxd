//! Backend feature audits ensure required DB capabilities are available.

#[cfg(feature = "postgres")]
use diesel::QueryableByName;
use diesel::result::QueryResult;
use diesel_async::RunQueryDsl;

#[cfg(feature = "sqlite")]
use super::connection::DbConnection;

/// Verify that `SQLite` supports features required by the application.
///
/// Specifically checks for the presence of the JSON1 extension and
/// recursive common table expressions. Dies when either capability is
/// missing.
///
/// # Errors
/// Returns any error produced by the test queries.
#[cfg(feature = "sqlite")]
#[must_use = "handle the result"]
pub async fn audit_sqlite_features(conn: &mut DbConnection) -> QueryResult<()> {
    use diesel::sql_query;

    // JSON1 extension: json() function must exist
    sql_query("SELECT json('{}')").execute(conn).await?;

    // Recursive CTE support
    sql_query(
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 1) SELECT * FROM c",
    )
    .execute(conn)
    .await?;

    Ok(())
}

/// Verify that the Postgres server meets application requirements.
///
/// Checks that the connected `PostgreSQL` server version is at least 14.
/// Executes a version query and parses the result, returning an error if the version is unsupported
/// or cannot be determined.
///
/// # Returns
///
/// - `Ok(())` if the server version is 14 or higher.
/// - An error if the version is below 14 or cannot be parsed.
///
/// # Examples
///
/// ```
/// # use mxd::db::audit_postgres_features;
/// # async fn check(conn: &mut diesel_async::AsyncPgConnection) {
/// let result = audit_postgres_features(conn).await;
/// assert!(result.is_ok());
/// # }
/// ```
///
/// # Errors
/// Returns any error produced by the version query or if the version string cannot be parsed.
#[cfg(feature = "postgres")]
#[must_use = "handle the result"]
pub async fn audit_postgres_features(
    conn: &mut diesel_async::AsyncPgConnection,
) -> QueryResult<()> {
    use diesel::{result::Error as DieselError, sql_query, sql_types::Text};

    #[derive(QueryableByName)]
    struct PgVersion {
        #[diesel(sql_type = Text)]
        version: String,
    }

    let row: PgVersion = sql_query("SELECT version()").get_result(conn).await?;

    let major = row
        .version
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.split('.').next())
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(|| {
            DieselError::QueryBuilderError(Box::new(std::io::Error::other(format!(
                "unable to parse postgres version: {}",
                row.version
            ))))
        })?;

    if major < 14 {
        return Err(DieselError::QueryBuilderError(Box::new(
            std::io::Error::other(format!(
                "postgres version {major} is not supported (require >= 14)"
            )),
        )));
    }

    Ok(())
}
