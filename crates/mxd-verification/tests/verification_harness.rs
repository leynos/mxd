//! Shared verification harness for session gating integration tests.

use std::collections::BTreeSet;

use mxd_verification::session_model::SessionModel;
use stateright::{Checker, Expectation, HasDiscoveries, Model};

/// Minimum states expected to demonstrate non-trivial exploration.
pub const MIN_STATE_COUNT: usize = 10;
const TARGET_MAX_DEPTH: usize = 6;
const TARGET_STATE_COUNT: usize = 1500;

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

/// Summary of a session-model verification run.
#[derive(Clone, Copy, Debug)]
pub struct VerificationOutcome {
    /// Number of unique states explored by the checker.
    pub unique_state_count: usize,
    /// Reachability properties that were not discovered.
    pub missing_reachability: usize,
    /// Safety properties that produced counterexamples.
    pub safety_counterexamples: usize,
}

impl VerificationOutcome {
    /// Returns `true` when all reachability and safety checks pass.
    #[must_use]
    pub const fn is_verified(&self) -> bool {
        self.missing_reachability == 0 && self.safety_counterexamples == 0
    }
}

/// Runs the session model with conservative bounds and summarizes the result.
///
/// # Examples
///
/// ```rust,ignore
/// use mxd_verification::session_model::SessionModel;
/// use crate::verification_harness::verify_session_model;
///
/// let outcome = verify_session_model(&SessionModel::default());
/// assert!(outcome.is_verified());
/// ```
#[must_use]
pub fn verify_session_model(model: &SessionModel) -> VerificationOutcome {
    let names = property_names(model);
    let checker = model
        .clone()
        .checker()
        .target_max_depth(TARGET_MAX_DEPTH)
        .target_state_count(TARGET_STATE_COUNT)
        .finish_when(HasDiscoveries::AllOf(names.reachability.clone()))
        .spawn_bfs()
        .join();
    let discoveries: BTreeSet<_> = checker.discoveries().keys().copied().collect();
    let missing_reachability = names.reachability.difference(&discoveries).count();
    let safety_counterexamples = names.safety.intersection(&discoveries).count();
    VerificationOutcome {
        unique_state_count: checker.unique_state_count(),
        missing_reachability,
        safety_counterexamples,
    }
}
