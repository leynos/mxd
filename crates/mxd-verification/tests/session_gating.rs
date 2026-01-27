//! Verification harness for the Stateright session gating model.

mod verification_harness;

use mxd_verification::session_model::SessionModel;
use rstest::rstest;
use verification_harness::{MIN_STATE_COUNT, verify_session_model};

#[rstest]
#[case("default config", SessionModel::default())]
#[case("single client", SessionModel::minimal())]
#[case("concurrent clients", SessionModel::with_clients(3))]
fn session_model_verifies(#[case] name: &str, #[case] model: SessionModel) {
    let summary = verify_session_model(&model);
    assert!(
        summary.is_verified(),
        "case {name}: reachability missing: {}, safety counterexamples: {}",
        summary.missing_reachability,
        summary.safety_counterexamples
    );
    assert!(
        summary.unique_state_count >= MIN_STATE_COUNT,
        "case {name}: expected >= {MIN_STATE_COUNT} states, got {}",
        summary.unique_state_count
    );
}
