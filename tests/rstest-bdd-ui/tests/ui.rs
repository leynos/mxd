//! Compile-time regressions for the scenario macro's migration surface.

#[test]
fn scenario_selection_and_harness_compile_contracts_are_stable() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass_named_harness.rs");
    cases.pass("tests/ui/pass_mutable_world_async_steps.rs");
    cases.compile_fail("tests/ui/fail_missing_scenario.rs");
}
