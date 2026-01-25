//! Verification harness for the Stateright session gating model.

use mxd_verification::session_model::SessionModel;
use stateright::{Checker, Model};

const MIN_STATE_COUNT: usize = 100;

#[derive(Clone, Copy, Debug)]
struct VerificationSummary {
    is_done: bool,
    unique_state_count: usize,
}

fn verify_model(model: &SessionModel) -> VerificationSummary {
    let checker = model.clone().checker().spawn_bfs().join();
    checker.assert_properties();
    VerificationSummary {
        is_done: checker.is_done(),
        unique_state_count: checker.unique_state_count(),
    }
}

#[test]
fn session_model_verifies_with_default_config() {
    let model = SessionModel::default();
    let summary = verify_model(&model);
    assert!(summary.is_done);
}

#[test]
fn session_model_explores_nontrivial_state_space() {
    let model = SessionModel::default();
    let summary = verify_model(&model);
    assert!(
        summary.unique_state_count >= MIN_STATE_COUNT,
        "expected >= {MIN_STATE_COUNT} states, got {}",
        summary.unique_state_count
    );
}

#[test]
fn session_model_verifies_single_client() {
    let model = SessionModel::minimal();
    let summary = verify_model(&model);
    assert!(summary.is_done);
}

#[test]
fn session_model_verifies_concurrent_clients() {
    let model = SessionModel::with_clients(3);
    let summary = verify_model(&model);
    assert!(summary.is_done);
}
