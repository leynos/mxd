//! Verification harness for the Stateright session gating model.

use std::collections::BTreeSet;

use mxd_verification::session_model::SessionModel;
use stateright::{Checker, Expectation, HasDiscoveries, Model};

const MIN_STATE_COUNT: usize = 10;
const TARGET_MAX_DEPTH: usize = 6;
const TARGET_STATE_COUNT: usize = 1500;

#[derive(Clone, Copy, Debug)]
struct VerificationSummary {
    unique_state_count: usize,
    missing_reachability: usize,
    safety_counterexamples: usize,
}

impl VerificationSummary {
    const fn is_verified(&self) -> bool {
        self.missing_reachability == 0 && self.safety_counterexamples == 0
    }
}

#[derive(Clone, Debug)]
struct PropertyNames {
    safety: BTreeSet<&'static str>,
    reachability: BTreeSet<&'static str>,
}

fn property_names(model: &SessionModel) -> PropertyNames {
    let mut safety = BTreeSet::new();
    let mut reachability = BTreeSet::new();
    for property in model.properties() {
        match property.expectation {
            Expectation::Always | Expectation::Eventually => {
                safety.insert(property.name);
            }
            Expectation::Sometimes => {
                reachability.insert(property.name);
            }
        }
    }
    PropertyNames {
        safety,
        reachability,
    }
}

fn verify_model(model: &SessionModel) -> VerificationSummary {
    let property_names = property_names(model);
    let checker = model
        .clone()
        .checker()
        .target_max_depth(TARGET_MAX_DEPTH)
        .target_state_count(TARGET_STATE_COUNT)
        .finish_when(HasDiscoveries::AllOf(property_names.reachability.clone()))
        .spawn_bfs()
        .join();
    let discoveries: BTreeSet<_> = checker.discoveries().keys().copied().collect();
    let missing_reachability = property_names.reachability.difference(&discoveries).count();
    let safety_counterexamples = property_names.safety.intersection(&discoveries).count();
    VerificationSummary {
        unique_state_count: checker.unique_state_count(),
        missing_reachability,
        safety_counterexamples,
    }
}

#[test]
fn session_model_verifies_with_default_config() {
    let model = SessionModel::default();
    let summary = verify_model(&model);
    assert!(
        summary.is_verified(),
        "reachability missing: {}, safety counterexamples: {}",
        summary.missing_reachability,
        summary.safety_counterexamples
    );
    assert!(summary.unique_state_count >= MIN_STATE_COUNT);
}

#[test]
fn session_model_explores_nontrivial_state_space() {
    let model = SessionModel::default();
    let summary = verify_model(&model);
    assert!(
        summary.is_verified(),
        "reachability missing: {}, safety counterexamples: {}",
        summary.missing_reachability,
        summary.safety_counterexamples
    );
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
    assert!(
        summary.is_verified(),
        "reachability missing: {}, safety counterexamples: {}",
        summary.missing_reachability,
        summary.safety_counterexamples
    );
    assert!(summary.unique_state_count >= MIN_STATE_COUNT);
}

#[test]
fn session_model_verifies_concurrent_clients() {
    let model = SessionModel::with_clients(3);
    let summary = verify_model(&model);
    assert!(
        summary.is_verified(),
        "reachability missing: {}, safety counterexamples: {}",
        summary.missing_reachability,
        summary.safety_counterexamples
    );
    assert!(summary.unique_state_count >= MIN_STATE_COUNT);
}
