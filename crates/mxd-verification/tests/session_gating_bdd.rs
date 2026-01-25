//! Behaviour-driven tests for the session gating verification model.

use std::{cell::RefCell, collections::BTreeSet};

use mxd_verification::session_model::SessionModel;
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use stateright::{Checker, Expectation, HasDiscoveries, Model};

const TARGET_MAX_DEPTH: usize = 6;
const TARGET_STATE_COUNT: usize = 1500;

#[derive(Clone, Copy, Debug, Default)]
struct VerificationResult {
    ran: bool,
    properties_verified: bool,
    unique_state_count: usize,
}

struct VerificationWorld {
    model: RefCell<SessionModel>,
    result: RefCell<Option<VerificationResult>>,
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
        let property_names = property_names(&model);
        let checker = model
            .checker()
            .target_max_depth(TARGET_MAX_DEPTH)
            .target_state_count(TARGET_STATE_COUNT)
            .finish_when(HasDiscoveries::AllOf(property_names.reachability.clone()))
            .spawn_bfs()
            .join();
        let discoveries: BTreeSet<_> = checker.discoveries().keys().copied().collect();
        let properties_verified = property_names.reachability.is_subset(&discoveries)
            && property_names.safety.is_disjoint(&discoveries);
        let result = VerificationResult {
            ran: true,
            properties_verified,
            unique_state_count: checker.unique_state_count(),
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
    assert!(world.result().properties_verified);
}

#[then("the model explores at least {count} states")]
fn then_state_space_size(world: &VerificationWorld, count: usize) {
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
