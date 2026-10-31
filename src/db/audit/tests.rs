//! Validator and probe tests for the `SQLite` capability audit.

use diesel_async::AsyncConnection;
use rstest::rstest;

use super::{
    Capability,
    DieselError,
    audit_sqlite_features,
    ensure_supported_sqlite_version,
    fetch_json_each_rows,
    fetch_recursive_cte_rows,
    fetch_sqlite_version,
    test_seam,
    validate_json_each_rows,
    validate_recursive_cte_rows,
};
use crate::db::DbConnection;

/// Assert that a validator rejected its input with an error naming the feature
/// that `capability` stands for, so a failure message always says which check
/// broke.
///
/// Taking the capability rather than a `&str` keeps the expected wording in
/// one place: [`Capability::describe`] is already pinned by
/// `capability_contract_is_pinned`, so this helper cannot drift away from the
/// text an operator will actually see.
fn assert_rejected_naming(result: Result<(), DieselError>, capability: Capability) {
    let expected = capability.describe();
    let error = result.expect_err("unexpected probe rows must be rejected");
    assert!(
        error.to_string().contains(expected),
        "error must name {expected}: {error}"
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
    assert_rejected_naming(validate_json_each_rows(&rows), Capability::Json1JsonEach);
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
    assert_rejected_naming(validate_recursive_cte_rows(rows), Capability::RecursiveCte);
}

/// A rejected row set must not have its rows copied into the error text.
///
/// `report_probe_failure` puts the whole error into a `warn!` field, so any row
/// value present here would be written to the log. The probe rows are a
/// fixed constant today, but the contract is that this module never treats
/// database output as log-safe, and a future probe over real data would
/// otherwise inherit the leak silently. Distinctive sentinel values prove the
/// assertion can fail: a validator that formatted its rows would reproduce
/// them here.
#[rstest]
#[case(&[(0, "SENTINEL_JSON_ROW")])]
#[case(&[(0, "a"), (1, "b"), (2, "SENTINEL_JSON_ROW")])]
fn rejected_json_each_rows_are_not_echoed(#[case] pairs: &[(i32, &str)]) {
    let rows = json_each_rows(pairs);
    let error = validate_json_each_rows(&rows).expect_err("unexpected rows must be rejected");
    assert!(
        !error.to_string().contains("SENTINEL_JSON_ROW"),
        "row values must not reach the error text: {error}"
    );
}

/// The recursive CTE validator must not echo rejected rows either.
#[rstest]
#[case(&[1, 2, 424_242])]
#[case(&[424_242])]
fn rejected_recursive_cte_rows_are_not_echoed(#[case] rows: &[i32]) {
    let error = validate_recursive_cte_rows(rows).expect_err("unexpected rows must be rejected");
    assert!(
        !error.to_string().contains("424242"),
        "row values must not reach the error text: {error}"
    );
}

/// The operational contract for every capability: variant, telemetry label,
/// operator-facing description.
///
/// Declared as data rather than spelled inline so the two views of the same
/// capability cannot drift apart, and so the table is the single place a
/// maintainer edits when the set changes. Both the label and the description
/// are observable outside this crate — the label is a `tracing` field value
/// and the description is the tail of the user-visible start-up error — which
/// is why both are pinned rather than derived.
const CAPABILITY_CONTRACT: &[(Capability, &str, &str)] = &[
    (Capability::Version, "version", "sqlite version"),
    (
        Capability::Json1JsonEach,
        "json1_json_each",
        "SQLite JSON1 json_each",
    ),
    (
        Capability::RecursiveCte,
        "recursive_cte",
        "SQLite recursive CTE",
    ),
];

/// Every capability matches the pinned contract table.
#[test]
fn capability_contract_is_pinned() {
    for (capability, label, description) in CAPABILITY_CONTRACT {
        assert_eq!(capability.label(), *label);
        assert_eq!(capability.describe(), *description);
    }
}

/// No two capabilities share a telemetry label.
///
/// A duplicate would merge two capabilities into one series and make the
/// per-capability breakdown meaningless. Adding a variant is already caught by
/// the compiler, because `label` and `describe` both match exhaustively, so
/// distinctness is the one property of the set that nothing else enforces.
#[test]
fn capability_labels_are_distinct() {
    let mut labels: Vec<_> = CAPABILITY_CONTRACT
        .iter()
        .map(|(_, label, _)| *label)
        .collect();
    labels.sort_unstable();
    let total = labels.len();
    labels.dedup();

    assert_eq!(labels.len(), total, "two capabilities share a label");
}

/// Run the three probe queries against a real library.
///
/// The validator tests above supply their own rows, so they cannot show
/// that the SQL in this module executes or that its columns deserialize:
/// a malformed probe string or a wrong `sql_type` attribute would leave
/// every one of them passing. This test drives the fetch helpers against
/// the linked library instead. The probes touch no table, so an empty
/// in-memory connection is enough and no migration is required.
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

/// The audit reports an injected failure through its public signature.
///
/// `probes_execute_against_the_linked_library` asserts only success, so
/// `audit_sqlite_features` could stop calling `run_sqlite_probes` entirely and
/// still pass. This test is the negative control: a failure that only the
/// probes can observe must reach the caller, naming the capability.
///
/// The seam is read before the first `await` and is thread-local, so this test
/// needs no serialization against the success test above.
#[rstest]
#[case(Capability::Version)]
#[case(Capability::Json1JsonEach)]
#[case(Capability::RecursiveCte)]
#[tokio::test]
async fn audit_reports_injected_probe_failure(#[case] capability: Capability) {
    let mut conn = DbConnection::establish(":memory:")
        .await
        .expect("in-memory connection must open");

    let error = {
        // Bind the guard so the override lifts when this block ends, even if
        // the assertion below fails.
        let _injection = test_seam::inject_failure(capability);
        audit_sqlite_features(&mut conn)
            .await
            .expect_err("an injected failure must reach the caller")
    };

    assert!(
        error.to_string().contains(capability.describe()),
        "the error must name {}: {error}",
        capability.describe()
    );
}

/// The override lifts when its guard drops.
///
/// Without this, the tests above would pass even if the guard leaked, and a
/// later test on the same thread would fail for an unrelated reason.
#[tokio::test]
async fn audit_passes_once_the_injected_failure_is_cleared() {
    let mut conn = DbConnection::establish(":memory:")
        .await
        .expect("in-memory connection must open");

    drop(test_seam::inject_failure(Capability::Version));
    audit_sqlite_features(&mut conn)
        .await
        .expect("the audit must pass once the override is gone");
}
