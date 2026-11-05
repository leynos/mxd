//! The row-returning `SQLite` probes and the rows each must produce.
//!
//! Two of the three capability probes return rows rather than a scalar, and
//! each is paired with the constant result set the audit accepts. Keeping the
//! query text, the expected rows, and the fetch helper for a probe in one place
//! means a change to a probe query and a change to its expectation are made
//! against each other, in a file that holds nothing else.
//!
//! The expected rows live here rather than beside the validators so that the
//! validators, which are the audit's decision logic, stay readable without the
//! query text interleaved between them.

use diesel::{
    QueryableByName,
    result::Error as DieselError,
    sql_query,
    sql_types::{Integer, Text},
};
use diesel_async::RunQueryDsl;

use super::{Capability, DbConnection, probe_failure};

/// Probe that exercises the JSON1 `json_each` table-valued function.
///
/// `json_each` is what the file-node path resolver uses to split a path into
/// segments, so the audit must prove the function runs and returns usable
/// rows, not merely that the JSON1 scalar functions exist.
pub(super) const JSON_EACH_PROBE_SQL: &str = concat!(
    "SELECT key, CAST(value AS TEXT) AS value ",
    "FROM json_each('[\"a\",\"b\",\"c\"]') ORDER BY key"
);

/// Probe that exercises recursive common table expression support.
pub(super) const RECURSIVE_CTE_PROBE_SQL: &str = concat!(
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 ",
    "FROM c WHERE x < 3) SELECT x FROM c ORDER BY x"
);

/// Rows the JSON1 probe must produce, in order.
pub(super) const EXPECTED_JSON_EACH_ROWS: [(i32, &str); 3] = [(0, "a"), (1, "b"), (2, "c")];

/// Rows the recursive CTE probe must produce, in order.
pub(super) const EXPECTED_RECURSIVE_CTE_ROWS: [i32; 3] = [1, 2, 3];

/// One row of the `json_each` result, in the shape the probe query selects.
#[derive(QueryableByName)]
struct JsonEachRow {
    #[diesel(sql_type = Integer)]
    key: i32,
    #[diesel(sql_type = Text)]
    value: String,
}

/// One row of the recursive CTE result.
#[derive(QueryableByName)]
struct RecursiveCteRow {
    #[diesel(sql_type = Integer)]
    x: i32,
}

/// Read the rows produced by the JSON1 `json_each` probe.
///
/// Under test a result injection can stand in for the query, so that a test can
/// drive an incorrect result set through the validator that consumes it. The
/// read happens before the query is awaited and stays on the caller's thread
/// when the test runtime is current-thread.
pub(super) async fn fetch_json_each_rows(
    conn: &mut DbConnection,
) -> Result<Vec<(i32, String)>, DieselError> {
    #[cfg(test)]
    if let Some(rows) = super::test_seam::injected_json_each_rows() {
        return Ok(rows);
    }

    sql_query(JSON_EACH_PROBE_SQL)
        .load::<JsonEachRow>(conn)
        .await
        .map(|rows| rows.into_iter().map(|row| (row.key, row.value)).collect())
        .map_err(|error| probe_failure(Capability::Json1JsonEach, &error.to_string()))
}

/// Read the rows produced by the recursive CTE probe.
///
/// As with the JSON1 fetch, a test result injection can stand in for the query
/// so the validator downstream of it can be exercised through the public audit.
pub(super) async fn fetch_recursive_cte_rows(
    conn: &mut DbConnection,
) -> Result<Vec<i32>, DieselError> {
    #[cfg(test)]
    if let Some(rows) = super::test_seam::injected_recursive_cte_rows() {
        return Ok(rows);
    }

    sql_query(RECURSIVE_CTE_PROBE_SQL)
        .load::<RecursiveCteRow>(conn)
        .await
        .map(|rows| rows.into_iter().map(|row| row.x).collect())
        .map_err(|error| probe_failure(Capability::RecursiveCte, &error.to_string()))
}
