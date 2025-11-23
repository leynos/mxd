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
#[allow(unfulfilled_lint_expectations)]
#[expect(unused_braces, reason = "Rust lint false positive")]
fn world() -> RuntimeWorld { RuntimeWorld::new() }

#[given("the runtime selection is computed")]
fn given_runtime(world: &RuntimeWorld) { world.compute(); }

#[allow(clippy::needless_pass_by_value)]
#[then("the active runtime is \"{runtime}\"")]
fn then_runtime(world: &RuntimeWorld, runtime: String) {
    let expected = match runtime.as_str() {
        "legacy" => NetworkRuntime::Legacy,
        "wireframe" => NetworkRuntime::Wireframe,
        other => panic!("unexpected runtime '{other}'"),
    };

    assert_eq!(world.runtime(), expected);
}

#[cfg(feature = "legacy-networking")]
#[scenario(path = "tests/features/runtime_selection.feature", index = 0)]
fn legacy_runtime_enabled(world: RuntimeWorld) { let _ = world; }

#[cfg(not(feature = "legacy-networking"))]
#[scenario(path = "tests/features/runtime_selection.feature", index = 1)]
fn legacy_runtime_disabled(world: RuntimeWorld) { let _ = world; }
