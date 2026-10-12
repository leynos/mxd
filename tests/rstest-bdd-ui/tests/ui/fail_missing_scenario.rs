//! Compile-fail fixture for the trybuild suite in `tests/ui.rs`.
//! It checks that an unknown title is rejected against `feature.feature`.

use rstest_bdd_macros::{given, scenario};

#[given("a migration UI step")]
fn migration_ui_step() {}

#[scenario(
    path = "../../../../tests/rstest-bdd-ui/tests/ui/feature.feature",
    name = "missing scenario",
    harness = rstest_bdd_harness_tokio::TokioHarness,
)]
fn missing_scenario() {}

fn main() {}
