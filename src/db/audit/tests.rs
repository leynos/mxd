//! Validator and probe tests for the `SQLite` capability audit.

use diesel_async::AsyncConnection;
use rstest::rstest;

use super::{
    Capability,
    DieselError,
    ensure_supported_sqlite_version,
    fetch_json_each_rows,
    fetch_recursive_cte_rows,
    fetch_sqlite_version,
    validate_json_each_rows,
    validate_recursive_cte_rows,
};
use crate::db::DbConnection;

/// Assert that a validator rejected its input with an error naming
/// `capability`, so a failure message always says which check broke.
fn assert_rejected_naming(result: Result<(), DieselError>, capability: &str) {
    let error = result.expect_err("unexpected probe rows must be rejected");
    assert!(
        error.to_string().contains(capability),
        "error must name the capability {capability}: {error}"
    );
}

#[rstest]
#[case("3.35.0")]
#[case("3.45.1")]
// Deliberate parser-tolerance case: `sqlite_version()` returns a bare
// triple, and the build date belongs to `sqlite_source_id()`. Trailing
// text must still be ignored rather than treated as unparsable.
#[case("3.46.1 2024-08-13 09:16:08")]
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
    assert_rejected_naming(
        validate_json_each_rows(&rows),
        "SQLite JSON1 json_each probe failed",
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
    assert_rejected_naming(
        validate_recursive_cte_rows(rows),
        "SQLite recursive CTE probe failed",
    );
}

/// Every capability keeps a distinct, stable telemetry label.
///
/// The labels are emitted as `tracing` field values, so they are an operational
/// contract: an operator alerting on `capability = "recursive_cte"` breaks
/// silently if a rename lands. The labels must also stay distinct, because two
/// capabilities sharing a label would merge into one series and make the
/// per-capability breakdown meaningless. This test is the only place that can
/// fail when the set changes, since the variants are private.
#[rstest]
#[case(Capability::Version, "version")]
#[case(Capability::Json1JsonEach, "json1_json_each")]
#[case(Capability::RecursiveCte, "recursive_cte")]
fn capability_labels_are_stable(#[case] capability: Capability, #[case] expected: &str) {
    assert_eq!(capability.label(), expected);
}

/// Every capability is describable and its description names the feature.
#[rstest]
#[case(Capability::Version, "sqlite version")]
#[case(Capability::Json1JsonEach, "SQLite JSON1 json_each")]
#[case(Capability::RecursiveCte, "SQLite recursive CTE")]
fn capability_descriptions_name_the_feature(
    #[case] capability: Capability,
    #[case] expected: &str,
) {
    assert_eq!(capability.describe(), expected);
}

/// Run the three probe queries against a real library.
///
/// The validator tests above supply their own rows, so they cannot show
/// that the SQL in this module executes or that its columns deserialize:
/// a malformed probe string or a wrong `sql_type` attribute would leave
/// every one of them passing. This test drives the fetch helpers against
/// the linked library instead. The probes touch no table, so an empty
/// in-memory connection is enough and no migration is required.
#[rstest]
#[tokio::test]
async fn probes_execute_against_the_linked_library() {
    let mut conn = DbConnection::establish(":memory:")
        .await
        .expect("in-memory connection must open");

    let version = fetch_sqlite_version(&mut conn)
        .await
        .expect("version probe must execute");
    ensure_supported_sqlite_version(&version)
        .expect("the linked library must meet the documented floor");

    let json_rows = fetch_json_each_rows(&mut conn)
        .await
        .expect("json_each probe must execute");
    validate_json_each_rows(&json_rows).expect("json_each must return the expected rows");

    let cte_rows = fetch_recursive_cte_rows(&mut conn)
        .await
        .expect("recursive CTE probe must execute");
    validate_recursive_cte_rows(&cte_rows).expect("recursive CTE must return the expected rows");
}
