//! Behaviour-driven tests for the session gating verification model.

mod verification_harness;

use std::cell::RefCell;

use mxd_verification::session_model::SessionModel;
use rstest::fixture;
use rstest_bdd_macros::{given, scenarios, then, when};
use stateright::Model;
use verification_harness::{MIN_STATE_COUNT, verify_session_model};

#[derive(Debug)]
enum VerificationError {
    AlreadyExecuted,
    NotExecuted,
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExecuted => write!(f, "verification already executed"),
            Self::NotExecuted => write!(f, "verification not executed"),
        }
    }
}

impl std::error::Error for VerificationError {}

#[derive(Clone, Copy, Debug, Default)]
struct VerificationResult {
    ran: bool,
    properties_verified: bool,
    unique_state_count: usize,
    missing_reachability: usize,
    safety_counterexamples: usize,
}

struct VerificationWorld {
    model: RefCell<SessionModel>,
    result: RefCell<Option<VerificationResult>>,
}

impl VerificationWorld {
    fn new() -> Self {
        Self {
            model: RefCell::new(SessionModel::default()),
            result: RefCell::new(None),
        }
    }

    fn set_model(&self, model: SessionModel) { *self.model.borrow_mut() = model; }

    fn verify(&self) {
        let model = self.model.borrow().clone();
        let outcome = verify_session_model(&model);
        let result = VerificationResult {
            ran: true,
            properties_verified: outcome.is_verified(),
            unique_state_count: outcome.unique_state_count,
            missing_reachability: outcome.missing_reachability,
            safety_counterexamples: outcome.safety_counterexamples,
        };
        self.result.replace(Some(result));
    }

    fn result(&self) -> Result<VerificationResult, VerificationError> {
        self.result.borrow().ok_or(VerificationError::NotExecuted)
    }
}

#[fixture]
fn world() -> VerificationWorld {
    #[expect(
        clippy::allow_attributes,
        reason = "fixture macro expansion triggers unused-braces on expression bodies"
    )]
    #[allow(
        unused_braces,
        reason = "fixture macro expansion triggers unused-braces on expression bodies"
    )]
    {
        VerificationWorld::new()
    }
}

fn ensure_unverified(world: &VerificationWorld) -> Result<(), Box<dyn std::error::Error>> {
    if world.result.borrow().is_some() {
        return Err(Box::new(VerificationError::AlreadyExecuted));
    }
    Ok(())
}

#[given("the session gating model uses default bounds")]
fn given_default_model(world: &VerificationWorld) -> Result<(), Box<dyn std::error::Error>> {
    ensure_unverified(world)?;
    // The Given step explicitly re-states the feature precondition, even
    // though the fixture starts with the default model.
    world.set_model(SessionModel::default());
    Ok(())
}

#[when("I verify the session gating model")]
fn when_verify_model(world: &VerificationWorld) -> Result<(), Box<dyn std::error::Error>> {
    ensure_unverified(world)?;
    world.verify();
    Ok(())
}

#[then("the verification completes")]
fn then_verification_completes(
    world: &VerificationWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(world.result()?.ran);
    Ok(())
}

#[then("the properties are satisfied")]
fn then_properties_satisfied(world: &VerificationWorld) -> Result<(), Box<dyn std::error::Error>> {
    let result = world.result()?;
    assert!(
        result.properties_verified,
        "reachability missing: {}, safety counterexamples: {}",
        result.missing_reachability, result.safety_counterexamples
    );
    Ok(())
}

#[then("the model explores at least {count} states")]
fn then_state_space_size(
    world: &VerificationWorld,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        count >= MIN_STATE_COUNT,
        "feature expectations should not undercut the harness minimum"
    );
    let result = world.result()?;
    assert!(
        result.unique_state_count >= count,
        "expected at least {count} states, got {}",
        result.unique_state_count
    );
    Ok(())
}

/// Assert a structural model property rather than a verification result.
///
/// The `ensure_unverified` guard ensures this step is only run while the
/// world still represents the model definition phase.
#[then("the model includes the out-of-order delivery property")]
fn then_out_of_order_property(world: &VerificationWorld) -> Result<(), Box<dyn std::error::Error>> {
    ensure_unverified(world)?;
    let properties = world.model.borrow().properties();
    assert!(
        properties
            .iter()
            .any(|property| property.name.contains("queued messages"))
    );
    Ok(())
}

scenarios!(
    "../../tests/features/session_gating_verification.feature",
    fixtures = [world: VerificationWorld]
);
