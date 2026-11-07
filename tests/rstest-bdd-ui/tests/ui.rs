//! Compile-time regressions for the scenario macro's migration surface.
//!
//! `trybuild` compiles pass fixtures with `cargo build --bin`, which type-checks
//! only the binary target and never the bodies of `#[test]` functions. A
//! scenario defined inside a fixture therefore pins nothing about the
//! macro-generated wrapper. The fixtures below are still valuable as
//! binary-level checks, but the wrapper contracts are pinned by the
//! `fixture_scenarios` module in this test crate, which `cargo test` compiles in
//! test mode.

#[test]
fn scenario_selection_and_harness_compile_contracts_are_stable() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass_named_harness.rs");
    cases.pass("tests/ui/pass_mutable_world_async_steps.rs");
    cases.compile_fail("tests/ui/fail_missing_scenario.rs");
}

/// Scenario wrappers pinned in test mode, where they are actually type-checked.
///
/// Feature paths here resolve against this package's manifest directory
/// (`tests/rstest-bdd-ui`), which is why they carry fewer `../` hops than the
/// trybuild fixtures. Those fixtures are compiled in a scratch crate nested
/// under `target/tests/trybuild/`, so the same file needs a longer relative
/// path from there. Both forms reach the same `.feature` files.
#[cfg(test)]
mod fixture_scenarios {
    use rstest::fixture;
    use rstest_bdd_macros::{given, scenario, then, when};

    /// World with plain owned state, mirroring `HandshakeWorld`.
    #[derive(Default)]
    struct World {
        recorded: Option<u32>,
    }

    // `rustfmt`'s `fn_single_line` keeps this a one-liner, and the `fixture`
    // expansion wraps the body in braces, so the lint is unavoidable here.
    // `allow` rather than `expect` for the same reason as `handshake_bdd.rs`:
    // rustc does not emit the paired `unfulfilled_lint_expectations` warning.
    #[expect(
        clippy::allow_attributes,
        reason = "rustc compiler does not emit expected lint"
    )]
    #[allow(unused_braces, reason = "rstest-bdd macro expansion produces braces")]
    #[fixture]
    fn world() -> World { World::default() }

    #[given("a mutable world")]
    fn given_world(world: &mut World) { world.recorded = None; }

    /// The await is what pins the pattern: the mutable borrow of `world` is
    /// held across a suspension point, so this fails to compile if the
    /// wrapper ever takes the world by value or re-borrows it eagerly.
    #[when("the world records a value")]
    async fn when_records(world: &mut World) {
        std::future::ready(()).await;
        world.recorded = Some(7);
    }

    #[then("the value is recorded")]
    fn then_recorded(world: &World) {
        assert_eq!(world.recorded, Some(7));
    }

    #[given("a migration UI step")]
    fn migration_ui_step() {}

    /// Named selection plus an external current-thread Tokio test.
    #[scenario(path = "tests/ui/mutable_world.feature", name = "async mutable world")]
    #[tokio::test(flavor = "current_thread")]
    async fn async_mutable_world(_world: World) {}

    /// Named selection with the explicit `TokioHarness`.
    #[scenario(
        path = "tests/ui/feature.feature",
        name = "named scenario",
        harness = rstest_bdd_harness_tokio::TokioHarness,
    )]
    fn named_scenario() {}
}
