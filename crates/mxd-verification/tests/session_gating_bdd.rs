//! Behaviour-driven tests for the session gating verification model.

mod verification_harness;

use std::cell::RefCell;

use mxd_verification::session_model::SessionModel;
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
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
fn world() -> Result<VerificationWorld, Box<dyn std::error::Error>> {
    let world = VerificationWorld::new();
    if world.result.borrow().is_some() {
        return Err(Box::new(VerificationError::AlreadyExecuted));
    }
    Ok(world)
}

fn resolve_world(
    world: &Result<VerificationWorld, Box<dyn std::error::Error>>,
) -> Result<&VerificationWorld, Box<dyn std::error::Error>> {
    world
        .as_ref()
        .map_err(|_| Box::new(VerificationError::AlreadyExecuted) as Box<dyn std::error::Error>)
}

fn ensure_unverified(
    world: &Result<VerificationWorld, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if resolve_world(world)?.result.borrow().is_some() {
        return Err(Box::new(VerificationError::AlreadyExecuted));
    }
    Ok(())
}

#[given("the session gating model uses default bounds")]
fn given_default_model(
    world: &Result<VerificationWorld, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_unverified(world)?;
    let resolved_world = resolve_world(world)?;
    resolved_world.set_model(SessionModel::default());
    Ok(())
}

#[when("I verify the session gating model")]
fn when_verify_model(
    world: &Result<VerificationWorld, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_unverified(world)?;
    let resolved_world = resolve_world(world)?;
    resolved_world.verify();
    Ok(())
}

#[then("the verification completes")]
fn then_verification_completes(
    world: &Result<VerificationWorld, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolved_world = resolve_world(world)?;
    assert!(resolved_world.result()?.ran);
    Ok(())
}

#[then("the properties are satisfied")]
fn then_properties_satisfied(
    world: &Result<VerificationWorld, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolved_world = resolve_world(world)?;
    let result = resolved_world.result()?;
    assert!(
        result.properties_verified,
        "reachability missing: {}, safety counterexamples: {}",
        result.missing_reachability, result.safety_counterexamples
    );
    Ok(())
}

#[then("the model explores at least {count} states")]
fn then_state_space_size(
    world: &Result<VerificationWorld, Box<dyn std::error::Error>>,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        count >= MIN_STATE_COUNT,
        "feature expectations should not undercut the harness minimum"
    );
    let resolved_world = resolve_world(world)?;
    let result = resolved_world.result()?;
    assert!(
        result.unique_state_count >= count,
        "expected at least {count} states, got {}",
        result.unique_state_count
    );
    Ok(())
}

#[then("the model includes the out-of-order delivery property")]
fn then_out_of_order_property(
    world: &Result<VerificationWorld, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_unverified(world)?;
    let resolved_world = resolve_world(world)?;
    let properties = resolved_world.model.borrow().properties();
    assert!(
        properties
            .iter()
            .any(|property| property.name.contains("queued messages"))
    );
    Ok(())
}

#[scenario(
    path = "../../tests/features/session_gating_verification.feature",
    index = 0
)]
fn session_model_verifies_default_bounds(
    world: Result<VerificationWorld, Box<dyn std::error::Error>>,
) {
    if let Err(error) = world {
        panic!("world fixture failed: {error}");
    }
}

#[scenario(
    path = "../../tests/features/session_gating_verification.feature",
    index = 1
)]
fn session_model_explores_state_space(
    world: Result<VerificationWorld, Box<dyn std::error::Error>>,
) {
    if let Err(error) = world {
        panic!("world fixture failed: {error}");
    }
}

#[scenario(
    path = "../../tests/features/session_gating_verification.feature",
    index = 2
)]
fn session_model_registers_out_of_order_property(
    world: Result<VerificationWorld, Box<dyn std::error::Error>>,
) {
    if let Err(error) = world {
        panic!("world fixture failed: {error}");
    }
}
