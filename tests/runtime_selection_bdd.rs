#![allow(
    unfulfilled_lint_expectations,
    reason = "test lint expectations may not all trigger"
)]
#![expect(missing_docs, reason = "test file")]
#![expect(clippy::expect_used, reason = "test assertions")]
#![expect(clippy::unwrap_used, reason = "test assertions")]
#![expect(clippy::panic_in_result_fn, reason = "test assertions")]
#![expect(clippy::big_endian_bytes, reason = "network protocol")]
#![expect(clippy::let_underscore_must_use, reason = "test cleanup")]

//! Behaviour-driven tests for runtime selection.
//!
//! Verifies `active_runtime()` and `NetworkRuntime` parsing against the
//! `runtime_selection.feature` scenarios for both legacy-enabled and
//! legacy-disabled builds.

#![expect(clippy::missing_const_for_fn, reason = "test fixture functions")]

use std::cell::RefCell;

use mxd::server::{NetworkRuntime, active_runtime};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then};

struct RuntimeWorld {
    runtime: RefCell<Option<NetworkRuntime>>,
}

impl RuntimeWorld {
    fn new() -> Self {
        Self {
            runtime: RefCell::new(None),
        }
    }

    fn compute(&self) { self.runtime.borrow_mut().replace(active_runtime()); }

    fn runtime(&self) -> NetworkRuntime { self.runtime.borrow().expect("runtime not computed") }
}

#[fixture]
fn world() -> RuntimeWorld {
    let world = RuntimeWorld::new();
    debug_assert!(world.runtime.borrow().is_none(), "world starts empty");
    world
}

#[given("the runtime selection is computed")]
fn given_runtime(world: &RuntimeWorld) { world.compute(); }

#[then("the active runtime is \"{runtime}\"")]
fn then_runtime(world: &RuntimeWorld, runtime: NetworkRuntime) {
    assert_eq!(world.runtime(), runtime);
}

#[cfg(feature = "legacy-networking")]
#[scenario(path = "tests/features/runtime_selection.feature", index = 0)]
fn legacy_runtime_enabled(world: RuntimeWorld) { let _ = world; }

#[cfg(not(feature = "legacy-networking"))]
#[scenario(path = "tests/features/runtime_selection.feature", index = 1)]
fn legacy_runtime_disabled(world: RuntimeWorld) { let _ = world; }
