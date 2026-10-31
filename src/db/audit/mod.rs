//! Backend feature audits ensure required DB capabilities are available.

#[cfg(any(feature = "postgres", feature = "sqlite"))]
use diesel::QueryableByName;
#[cfg(feature = "sqlite")]
use diesel::result::Error as DieselError;
use diesel::result::QueryResult;
use diesel_async::RunQueryDsl;
#[cfg(feature = "sqlite")]
use tracing::{debug, warn};

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
const JSON_EACH_PROBE_SQL: &str = concat!(
    "SELECT key, CAST(value AS TEXT) AS value ",
    "FROM json_each('[\"a\",\"b\",\"c\"]') ORDER BY key"
);

/// Probe that exercises recursive common table expression support.
#[cfg(feature = "sqlite")]
const RECURSIVE_CTE_PROBE_SQL: &str = concat!(
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 ",
    "FROM c WHERE x < 3) SELECT x FROM c ORDER BY x"
);

/// Rows the JSON1 probe must produce, in order.
#[cfg(feature = "sqlite")]
const EXPECTED_JSON_EACH_ROWS: [(i32, &str); 3] = [(0, "a"), (1, "b"), (2, "c")];

/// Rows the recursive CTE probe must produce, in order.
#[cfg(feature = "sqlite")]
const EXPECTED_RECURSIVE_CTE_ROWS: [i32; 3] = [1, 2, 3];

/// The fixed set of capabilities the audit probes.
///
/// The variants double as low-cardinality telemetry labels. Keep the set
/// closed: a capability derived from runtime input would make the emitted
/// labels unbounded. The displayed names are the ones an operator sees in the
/// startup error, so they name the user-facing feature rather than the probe.
#[cfg(feature = "sqlite")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Capability {
    /// The linked library reports a supported version.
    Version,
    /// The JSON1 `json_each` table-valued function answers usefully.
    Json1JsonEach,
    /// A recursive common table expression answers usefully.
    RecursiveCte,
}

#[cfg(feature = "sqlite")]
impl Capability {
    /// Describe the capability as it appears in an operator-facing message.
    const fn describe(self) -> &'static str {
        match self {
            Self::Version => "sqlite version",
            Self::Json1JsonEach => "SQLite JSON1 json_each",
            Self::RecursiveCte => "SQLite recursive CTE",
        }
    }

    /// Name the capability as a stable, low-cardinality telemetry label.
    const fn label(self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Json1JsonEach => "json1_json_each",
            Self::RecursiveCte => "recursive_cte",
        }
    }
}

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
    run_sqlite_probes(conn)
        .await
        .inspect(|()| debug!("SQLite capability audit passed"))
        .map_err(|(capability, error)| report_probe_failure(capability, error))
}

/// Log a failed probe and pass its error through unchanged.
///
/// The caller already wraps this error with an entry-point context. Emitting the
/// capability as its own field keeps the failure machine-filterable without
/// parsing the message.
///
/// This event copies the whole error into the `error` field, so whatever the
/// error's `Display` produces reaches the log. The row validators therefore
/// keep queried rows out of their errors and report only a row count; the
/// driver errors raised by the fetch helpers describe the failure itself and
/// are safe to include.
#[cfg(feature = "sqlite")]
fn report_probe_failure(capability: Capability, error: DieselError) -> DieselError {
    warn!(
        capability = capability.label(),
        %error,
        "SQLite capability audit failed"
    );
    error
}

/// Run the three probes in order, stopping at the first failure.
///
/// Returns the failing capability alongside the error so the caller can emit
/// it as a telemetry label without re-deriving it from error text. The label is
/// deliberately separate from the error: `probe_failure` is the only place
/// that knows which probe ran, and threading the pair keeps that knowledge
/// local rather than re-parsed downstream.
///
/// # Errors
///
/// Returns the first failing [`Capability`] paired with its [`DieselError`],
/// and does not run the remaining probes. A version below the documented floor
/// or an unparsable version string fails the version probe; a query the linked
/// library cannot execute fails that query's probe; and rows that differ from
/// the expected constant fail the corresponding row validator.
#[cfg(feature = "sqlite")]
async fn run_sqlite_probes(conn: &mut DbConnection) -> Result<(), (Capability, DieselError)> {
    fetch_sqlite_version(conn)
        .await
        .and_then(|version| ensure_supported_sqlite_version(&version))
        .map_err(|error| (Capability::Version, error))?;

    fetch_json_each_rows(conn)
        .await
        .and_then(|rows| validate_json_each_rows(&rows))
        .map_err(|error| (Capability::Json1JsonEach, error))?;

    fetch_recursive_cte_rows(conn)
        .await
        .and_then(|rows| validate_recursive_cte_rows(&rows))
        .map_err(|error| (Capability::RecursiveCte, error))
}

/// Build the error reported when a capability check fails.
#[cfg(feature = "sqlite")]
fn probe_failure(capability: Capability, detail: &str) -> DieselError {
    DieselError::QueryBuilderError(Box::new(std::io::Error::other(format!(
        "{} probe failed: {detail}",
        capability.describe()
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
        .map_err(|error| probe_failure(Capability::Version, &error.to_string()))
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
        .map_err(|error| probe_failure(Capability::Json1JsonEach, &error.to_string()))
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
        .map_err(|error| probe_failure(Capability::RecursiveCte, &error.to_string()))
}

/// Parse and range-check the version string reported by `sqlite_version()`.
///
/// Accepts a leading `major.minor.patch` triple and ignores any trailing text.
/// `sqlite_version()` returns the bare triple, so trailing text should not
/// occur; the tolerance is defensive, so that a library whose version string
/// carries an unexpected suffix is still recognized rather than rejected as
/// unparsable. Returns the "unable to parse" error when no such prefix is
/// present, so that an unrecognized library is never mistaken for a supported
/// one.
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

/// Whether the JSON1 probe rows match the expected constant exactly, in order.
#[cfg(feature = "sqlite")]
fn json_each_rows_match(rows: &[(i32, String)]) -> bool {
    rows.len() == EXPECTED_JSON_EACH_ROWS.len()
        && rows.iter().zip(EXPECTED_JSON_EACH_ROWS.iter()).all(
            |((key, value), (expected_key, expected_value))| {
                key == expected_key && value == expected_value
            },
        )
}

/// Describe a row mismatch without disclosing the rows.
///
/// `report_probe_failure` copies this text into a `warn!` field, so anything
/// returned here is written to whatever sink the subscriber uses. The expected
/// output is a compile-time constant in this module and safe to name; the
/// rows actually read from the database are not, so only their count is
/// reported and the contents are described rather than reproduced.
#[cfg(feature = "sqlite")]
fn row_mismatch_detail(observed: usize, expected: usize) -> String {
    if observed == expected {
        return "row contents did not match the expected constant".to_owned();
    }

    format!("expected {expected} rows, observed {observed}")
}

/// Check that the JSON1 probe returned exactly the expected rows, in order.
#[cfg(feature = "sqlite")]
fn validate_json_each_rows(rows: &[(i32, String)]) -> Result<(), DieselError> {
    if json_each_rows_match(rows) {
        return Ok(());
    }

    let detail = row_mismatch_detail(rows.len(), EXPECTED_JSON_EACH_ROWS.len());
    Err(probe_failure(Capability::Json1JsonEach, &detail))
}

/// Check that the recursive CTE probe returned exactly the expected rows.
#[cfg(feature = "sqlite")]
fn validate_recursive_cte_rows(rows: &[i32]) -> Result<(), DieselError> {
    if rows == EXPECTED_RECURSIVE_CTE_ROWS {
        return Ok(());
    }

    let detail = row_mismatch_detail(rows.len(), EXPECTED_RECURSIVE_CTE_ROWS.len());
    Err(probe_failure(Capability::RecursiveCte, &detail))
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
mod tests;
