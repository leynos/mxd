//! Test-only failure injection for the `SQLite` capability probes.
//!
//! The three probes are pure `SQLite` expressions that read no table, so no
//! database state can make them fail on demand: even a file full of arbitrary
//! bytes answers `sqlite_version()` and `json_each`. Without an injection
//! point the audit's failure path could not be exercised at all, and a
//! regression that reduced [`super::audit_sqlite_features`] to a no-op would
//! leave every test passing.
//!
//! The override is a thread-local rather than a process-wide static so that
//! tests running concurrently cannot observe it. It is read once at the top of
//! `run_sqlite_probes`, before the first `await`, which keeps the read on the
//! thread that installed it even though the connection work itself is moved to
//! a blocking pool.
//!
//! This module is compiled only for the test build, so release binaries
//! contain none of it.

use std::cell::Cell;

use diesel::result::Error as DieselError;

use super::{Capability, probe_failure};

thread_local! {
    static INJECTED_FAILURE: Cell<Option<Capability>> = const { Cell::new(None) };
}

/// Clear the injected failure when the test that set it ends.
///
/// Resetting on drop rather than at the end of the test body means a failing
/// assertion cannot leak the override into a later test on the same thread.
#[must_use = "hold the guard for as long as the failure should be injected"]
pub(crate) struct InjectionGuard;

impl Drop for InjectionGuard {
    fn drop(&mut self) { INJECTED_FAILURE.with(|cell| cell.set(None)); }
}

/// Report the failure the current test asked for, if it asked for one.
pub(super) fn injected_failure() -> Option<(Capability, DieselError)> {
    INJECTED_FAILURE.with(Cell::get).map(|capability| {
        (
            capability,
            probe_failure(capability, "injected probe failure"),
        )
    })
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
