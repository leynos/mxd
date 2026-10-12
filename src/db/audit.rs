//! Backend feature audits ensure required DB capabilities are available.

#[cfg(any(feature = "postgres", feature = "sqlite"))]
use diesel::QueryableByName;
#[cfg(feature = "sqlite")]
use diesel::result::Error as DieselError;
use diesel::result::QueryResult;
use diesel_async::RunQueryDsl;

#[cfg(feature = "sqlite")]
use super::connection::DbConnection;

/// Minimum `SQLite` version the application requires.
///
/// The floor exists because the `sqlite` Cargo feature enables Diesel's
/// `returning_clauses_for_sqlite_3_35`, which emits `RETURNING` clauses that
/// older libraries reject at run time.
#[cfg(feature = "sqlite")]
const MINIMUM_SQLITE_VERSION: (u32, u32, u32) = (3, 35, 0);

/// Probe that exercises the JSON1 `json_each` table-valued function.
///
/// `json_each` is what the file-node path resolver uses to split a path into
/// segments, so the audit must prove the function runs and returns usable
/// rows, not merely that the JSON1 scalar functions exist.
#[cfg(feature = "sqlite")]
const JSON_EACH_PROBE_SQL: &str =
    "SELECT key, CAST(value AS TEXT) AS value FROM json_each('[\"a\",\"b\",\"c\"]') ORDER BY key";

/// Probe that exercises recursive common table expression support.
#[cfg(feature = "sqlite")]
const RECURSIVE_CTE_PROBE_SQL: &str = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 \
                                       FROM c WHERE x < 3) SELECT x FROM c ORDER BY x";

/// Rows the JSON1 probe must produce, in order.
#[cfg(feature = "sqlite")]
const EXPECTED_JSON_EACH_ROWS: [(i32, &str); 3] = [(0, "a"), (1, "b"), (2, "c")];

/// Rows the recursive CTE probe must produce, in order.
#[cfg(feature = "sqlite")]
const EXPECTED_RECURSIVE_CTE_ROWS: [i32; 3] = [1, 2, 3];

/// Verify that `SQLite` supports features required by the application.
///
/// The audit runs three checks, in order:
///
/// 1. The linked library reports a version at or above 3.35.0.
/// 2. The JSON1 `json_each` function returns the expected rows.
/// 3. A recursive common table expression returns the expected rows.
///
/// Each check runs a query and inspects its result, so that a library which
/// accepts the syntax but computes the wrong answer is still rejected.
/// A failing check returns an error naming the capability at fault.
///
/// # Errors
/// Returns an error if any capability is missing, if a probe query fails, or
/// if a probe returns rows other than the expected ones.
#[cfg(feature = "sqlite")]
#[must_use = "handle the result"]
pub async fn audit_sqlite_features(conn: &mut DbConnection) -> QueryResult<()> {
    ensure_supported_sqlite_version(&fetch_sqlite_version(conn).await?)?;
    validate_json_each_rows(&fetch_json_each_rows(conn).await?)?;
    validate_recursive_cte_rows(&fetch_recursive_cte_rows(conn).await?)
}

/// Build the error reported when a capability check fails.
#[cfg(feature = "sqlite")]
fn probe_failure(capability: &str, detail: &str) -> DieselError {
    DieselError::QueryBuilderError(Box::new(std::io::Error::other(format!(
        "{capability} probe failed: {detail}"
    ))))
}

/// Read the version string reported by the linked `SQLite` library.
#[cfg(feature = "sqlite")]
async fn fetch_sqlite_version(conn: &mut DbConnection) -> Result<String, DieselError> {
    use diesel::{sql_query, sql_types::Text};

    #[derive(QueryableByName)]
    struct SqliteVersion {
        #[diesel(sql_type = Text)]
        version: String,
    }

    sql_query("SELECT sqlite_version() AS version")
        .get_result::<SqliteVersion>(conn)
        .await
        .map(|row| row.version)
        .map_err(|error| probe_failure("sqlite version", &error.to_string()))
}

/// Read the rows produced by the JSON1 `json_each` probe.
#[cfg(feature = "sqlite")]
async fn fetch_json_each_rows(conn: &mut DbConnection) -> Result<Vec<(i32, String)>, DieselError> {
    use diesel::{
        sql_query,
        sql_types::{Integer, Text},
    };

    #[derive(QueryableByName)]
    struct JsonEachRow {
        #[diesel(sql_type = Integer)]
        key: i32,
        #[diesel(sql_type = Text)]
        value: String,
    }

    sql_query(JSON_EACH_PROBE_SQL)
        .load::<JsonEachRow>(conn)
        .await
        .map(|rows| rows.into_iter().map(|row| (row.key, row.value)).collect())
        .map_err(|error| probe_failure("SQLite JSON1 json_each", &error.to_string()))
}

/// Read the rows produced by the recursive CTE probe.
#[cfg(feature = "sqlite")]
async fn fetch_recursive_cte_rows(conn: &mut DbConnection) -> Result<Vec<i32>, DieselError> {
    use diesel::{sql_query, sql_types::Integer};

    #[derive(QueryableByName)]
    struct RecursiveCteRow {
        #[diesel(sql_type = Integer)]
        x: i32,
    }

    sql_query(RECURSIVE_CTE_PROBE_SQL)
        .load::<RecursiveCteRow>(conn)
        .await
        .map(|rows| rows.into_iter().map(|row| row.x).collect())
        .map_err(|error| probe_failure("SQLite recursive CTE", &error.to_string()))
}

/// Parse and range-check the version string reported by `sqlite_version()`.
///
/// Accepts a leading `major.minor.patch` triple and ignores any trailing text,
/// because `sqlite_version()` may append a build date. Returns the
/// "unable to parse" error when no such prefix is present, so that an
/// unrecognized library is never mistaken for a supported one.
#[cfg(feature = "sqlite")]
fn ensure_supported_sqlite_version(raw: &str) -> Result<(u32, u32, u32), DieselError> {
    let parsed = parse_version_triple(raw).ok_or_else(|| {
        DieselError::QueryBuilderError(Box::new(std::io::Error::other(format!(
            "unable to parse sqlite version: {raw}"
        ))))
    })?;

    if parsed < MINIMUM_SQLITE_VERSION {
        let (major, minor, patch) = parsed;
        return Err(DieselError::QueryBuilderError(Box::new(
            std::io::Error::other(format!(
                "sqlite version {major}.{minor}.{patch} is not supported (require >= 3.35.0)"
            )),
        )));
    }

    Ok(parsed)
}

/// Extract the leading `major.minor.patch` triple from a version string.
#[cfg(feature = "sqlite")]
fn parse_version_triple(raw: &str) -> Option<(u32, u32, u32)> {
    let mut fields = raw.split_whitespace().next()?.split('.');
    let major: u32 = fields.next()?.parse().ok()?;
    let minor: u32 = fields.next()?.parse().ok()?;
    let patch: u32 = fields.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Check that the JSON1 probe returned exactly the expected rows, in order.
#[cfg(feature = "sqlite")]
fn validate_json_each_rows(rows: &[(i32, String)]) -> Result<(), DieselError> {
    let is_expected = rows.len() == EXPECTED_JSON_EACH_ROWS.len()
        && rows.iter().zip(EXPECTED_JSON_EACH_ROWS.iter()).all(
            |((key, value), (expected_key, expected_value))| {
                key == expected_key && value == expected_value
            },
        );

    if is_expected {
        return Ok(());
    }

    Err(probe_failure(
        "SQLite JSON1 json_each",
        &format!("unexpected rows {rows:?}"),
    ))
}

/// Check that the recursive CTE probe returned exactly the expected rows.
#[cfg(feature = "sqlite")]
fn validate_recursive_cte_rows(rows: &[i32]) -> Result<(), DieselError> {
    if rows == EXPECTED_RECURSIVE_CTE_ROWS {
        return Ok(());
    }

    Err(probe_failure(
        "SQLite recursive CTE",
        &format!("unexpected rows {rows:?}"),
    ))
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

#[cfg(feature = "sqlite")]
#[cfg(test)]
mod sqlite_tests {
    //! Validator tests for the `SQLite` capability audit.

    use rstest::rstest;

    use super::{
        ensure_supported_sqlite_version,
        validate_json_each_rows,
        validate_recursive_cte_rows,
    };

    #[rstest]
    #[case("3.35.0")]
    #[case("3.45.1")]
    #[case(
        "3.46.1 2024-08-13 09:16:08 \
         c9c2ab54ba1f5f46360f1b4f35d849cd3f080e6fc2b6c60e91b16c63f69aalt1"
    )]
    fn version_accepts_supported(#[case] raw: &str) {
        let parsed = ensure_supported_sqlite_version(raw).expect("version must be accepted");
        assert!(parsed >= (3, 35, 0), "{raw} must meet the floor");
    }

    #[rstest]
    #[case("3.34.1", "sqlite version 3.34.1 is not supported (require >= 3.35.0)")]
    #[case("2.8.17", "sqlite version 2.8.17 is not supported (require >= 3.35.0)")]
    #[case("", "unable to parse sqlite version: ")]
    #[case("abc", "unable to parse sqlite version: abc")]
    #[case("3.46", "unable to parse sqlite version: 3.46")]
    fn version_rejects_unsupported(#[case] raw: &str, #[case] expected: &str) {
        let error =
            ensure_supported_sqlite_version(raw).expect_err("unsupported version must be rejected");
        assert!(
            error.to_string().contains(expected),
            "expected {error} to contain {expected}"
        );
    }

    /// Build a probe row vector from literal pairs.
    fn json_each_rows(pairs: &[(i32, &str)]) -> Vec<(i32, String)> {
        pairs
            .iter()
            .map(|(key, value)| (*key, (*value).to_owned()))
            .collect()
    }

    #[rstest]
    fn json_each_accepts_expected_rows() {
        let rows = json_each_rows(&[(0, "a"), (1, "b"), (2, "c")]);
        validate_json_each_rows(&rows).expect("expected probe rows must be accepted");
    }

    #[rstest]
    #[case(&[(0, "a"), (1, "b")])]
    #[case(&[(0, "a"), (1, "b"), (2, "d")])]
    #[case(&[(1, "a"), (2, "b"), (3, "c")])]
    #[case(&[(0, "c"), (1, "b"), (2, "a")])]
    #[case(&[(0, "a"), (1, "b"), (2, "c"), (3, "d")])]
    #[case(&[])]
    fn json_each_rejects_unexpected_rows(#[case] pairs: &[(i32, &str)]) {
        let rows = json_each_rows(pairs);
        let error =
            validate_json_each_rows(&rows).expect_err("unexpected probe rows must be rejected");
        assert!(
            error
                .to_string()
                .contains("SQLite JSON1 json_each probe failed"),
            "error must name the capability: {error}"
        );
    }

    #[rstest]
    fn recursive_cte_accepts_expected_rows() {
        validate_recursive_cte_rows(&[1, 2, 3]).expect("expected probe rows must be accepted");
    }

    #[rstest]
    #[case(&[1])]
    #[case(&[])]
    #[case(&[1, 2])]
    #[case(&[1, 2, 4])]
    #[case(&[3, 2, 1])]
    #[case(&[1, 2, 3, 4])]
    fn recursive_cte_rejects_unexpected_rows(#[case] rows: &[i32]) {
        let error =
            validate_recursive_cte_rows(rows).expect_err("unexpected probe rows must be rejected");
        assert!(
            error
                .to_string()
                .contains("SQLite recursive CTE probe failed"),
            "error must name the capability: {error}"
        );
    }
}
