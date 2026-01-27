//! Behaviour-driven tests for the session gating verification model.

mod verification_harness;

use std::cell::RefCell;

use mxd_verification::session_model::SessionModel;
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use stateright::Model;
use verification_harness::{MIN_STATE_COUNT, verify_session_model};

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

    fn result(&self) -> VerificationResult {
        self.result
            .borrow()
            .map_or_else(|| panic!("verification not executed"), |result| result)
    }
}

#[fixture]
fn world() -> VerificationWorld {
    let world = VerificationWorld::new();
    debug_assert!(
        world.result.borrow().is_none(),
        "verification results start empty"
    );
    world
}

#[given("the session gating model uses default bounds")]
fn given_default_model(world: &VerificationWorld) { world.set_model(SessionModel::default()); }

#[when("I verify the session gating model")]
fn when_verify_model(world: &VerificationWorld) { world.verify(); }

#[then("the verification completes")]
fn then_verification_completes(world: &VerificationWorld) {
    assert!(world.result().ran);
}

#[then("the properties are satisfied")]
fn then_properties_satisfied(world: &VerificationWorld) {
    let result = world.result();
    assert!(
        result.properties_verified,
        "reachability missing: {}, safety counterexamples: {}",
        result.missing_reachability, result.safety_counterexamples
    );
}

#[then("the model explores at least {count} states")]
fn then_state_space_size(world: &VerificationWorld, count: usize) {
    debug_assert!(
        count >= MIN_STATE_COUNT,
        "feature expectations should not undercut the harness minimum"
    );
    assert!(
        world.result().unique_state_count >= count,
        "expected at least {count} states, got {}",
        world.result().unique_state_count
    );
}

#[then("the model includes the out-of-order delivery property")]
fn then_out_of_order_property(world: &VerificationWorld) {
    let properties = world.model.borrow().properties();
    assert!(
        properties
            .iter()
            .any(|property| property.name.contains("queued messages"))
    );
}

#[scenario(
    path = "../../tests/features/session_gating_verification.feature",
    index = 0
)]
fn session_model_verifies_default_bounds(world: VerificationWorld) { let _ = world; }

#[scenario(
    path = "../../tests/features/session_gating_verification.feature",
    index = 1
)]
fn session_model_explores_state_space(world: VerificationWorld) { let _ = world; }

#[scenario(
    path = "../../tests/features/session_gating_verification.feature",
    index = 2
)]
fn session_model_registers_out_of_order_property(world: VerificationWorld) { let _ = world; }
