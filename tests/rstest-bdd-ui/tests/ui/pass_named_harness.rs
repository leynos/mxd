use rstest_bdd_macros::{given, scenario};

#[given("a migration UI step")]
fn migration_ui_step() {}

#[scenario(
    path = "../../../../tests/rstest-bdd-ui/tests/ui/feature.feature",
    name = "named scenario",
    harness = rstest_bdd_harness_tokio::TokioHarness,
)]
fn named_scenario() {}

fn main() {}
