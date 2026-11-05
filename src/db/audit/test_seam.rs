//! Test-only failure injection for the `SQLite` capability probes.
//!
//! The three probes are pure `SQLite` expressions that read no table, so no
//! database state can make them fail on demand: even a file full of arbitrary
//! bytes answers `sqlite_version()` and `json_each`. Without an injection
//! point the audit's failure path could not be exercised at all, and a
//! regression that reduced [`super::audit_sqlite_features`] to a no-op would
//! leave every test passing.
//!
//! There are two kinds of injection, and they prove different things.
//!
//! An *abort* injection stops the probe run before any query executes. It is
//! what an entry point above the audit needs, since those callers only have to
//! know that an audit failure aborts them, not which probe failed.
//!
//! A *result* injection instead substitutes the rows a fetch helper would have
//! returned, so the validator downstream of that fetch still runs. An abort
//! injection cannot show that: it returns from `run_sqlite_probes` before the
//! first fetch, leaving every validator unexercised through the public audit.
//! If a validator call were deleted, the abort-injected tests and the direct
//! validator tests would both still pass.
//!
//! Every override is a thread-local rather than a process-wide static so that
//! tests running concurrently cannot observe it. Abort injections are read
//! once at the top of `run_sqlite_probes`, before the first `await`; result
//! injections are read at the top of their own fetch helper, also before that
//! helper awaits. Both reads therefore stay on the thread that installed them,
//! provided the test runs on a current-thread runtime, which is what
//! `#[tokio::test]` creates by default. A result injection used from a
//! multi-threaded runtime could be read on a different polling thread after
//! the preceding probe had awaited.
//!
//! This module is compiled only for the test build, so release binaries
//! contain none of it.

use std::cell::{Cell, RefCell};

use diesel::result::Error as DieselError;

use super::{Capability, probe_failure};

thread_local! {
    static INJECTED_FAILURE: Cell<Option<Capability>> = const { Cell::new(None) };
    static INJECTED_JSON_EACH: RefCell<Option<Vec<(i32, String)>>> = const { RefCell::new(None) };
    static INJECTED_RECURSIVE_CTE: RefCell<Option<Vec<i32>>> = const { RefCell::new(None) };
}

/// Clear every injected override when the test that set it ends.
///
/// Resetting on drop rather than at the end of the test body means a failing
/// assertion cannot leak an override into a later test on the same thread. One
/// guard type covers all three overrides because a test installs at most one
/// of them and clearing an override that was never set is a no-op.
#[must_use = "hold the guard for as long as the override should be injected"]
pub(crate) struct InjectionGuard;

impl Drop for InjectionGuard {
    fn drop(&mut self) {
        INJECTED_FAILURE.with(|cell| cell.set(None));
        INJECTED_JSON_EACH.with(|cell| cell.borrow_mut().take());
        INJECTED_RECURSIVE_CTE.with(|cell| cell.borrow_mut().take());
    }
}

/// Report the abort the current test asked for, if it asked for one.
pub(super) fn injected_failure() -> Option<(Capability, DieselError)> {
    INJECTED_FAILURE.with(Cell::get).map(|capability| {
        (
            capability,
            probe_failure(capability, "injected probe failure"),
        )
    })
}

/// Take the JSON1 rows a test asked the fetch helper to report instead.
///
/// Taking rather than copying makes the override single-use, so it cannot
/// silently apply to a second audit run the same test makes.
pub(super) fn injected_json_each_rows() -> Option<Vec<(i32, String)>> {
    INJECTED_JSON_EACH.with(|cell| cell.borrow_mut().take())
}

/// Take the recursive CTE rows a test asked the fetch helper to report instead.
pub(super) fn injected_recursive_cte_rows() -> Option<Vec<i32>> {
    INJECTED_RECURSIVE_CTE.with(|cell| cell.borrow_mut().take())
}

/// Fail the next capability audit, as the version probe.
///
/// Entry points above the audit only need to know that an audit failure aborts
/// them, not which probe failed, so this hides [`Capability`] from callers in
/// other modules. The failure lifts when the returned guard drops.
pub(crate) fn fail_next_audit() -> InjectionGuard {
    INJECTED_FAILURE.with(|cell| cell.set(Some(Capability::Version)));
    InjectionGuard
}

/// Fail the next capability audit, naming `capability` in the reported error.
pub(super) fn inject_failure(capability: Capability) -> InjectionGuard {
    INJECTED_FAILURE.with(|cell| cell.set(Some(capability)));
    InjectionGuard
}

/// Make the JSON1 fetch helper report `rows` without querying.
pub(super) fn inject_json_each_rows(rows: Vec<(i32, String)>) -> InjectionGuard {
    INJECTED_JSON_EACH.with(|cell| *cell.borrow_mut() = Some(rows));
    InjectionGuard
}

/// Make the recursive CTE fetch helper report `rows` without querying.
pub(super) fn inject_recursive_cte_rows(rows: Vec<i32>) -> InjectionGuard {
    INJECTED_RECURSIVE_CTE.with(|cell| *cell.borrow_mut() = Some(rows));
    InjectionGuard
}
