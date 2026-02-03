//! Behaviour-driven tests for the wireframe server bootstrap.

use std::cell::RefCell;

use rstest::fixture;
use rstest_bdd::{assert_step_err, assert_step_ok};
use rstest_bdd_macros::{given, scenario, then, when};

use super::*;

struct BootstrapWorld {
    config: RefCell<AppConfig>,
    outcome: RefCell<Option<Result<WireframeBootstrap>>>,
}

impl BootstrapWorld {
    fn new() -> Self {
        Self {
            config: RefCell::new(AppConfig::default()),
            outcome: RefCell::new(None),
        }
    }

    fn set_bind(&self, bind: String) { self.config.borrow_mut().bind = bind; }

    fn bootstrap(&self) {
        let cfg = self.config.borrow().clone();
        let result = WireframeBootstrap::prepare(cfg);
        self.outcome.borrow_mut().replace(result);
    }
}

#[fixture]
fn world() -> BootstrapWorld {
    let world = BootstrapWorld::new();
    world.config.borrow_mut().bind = "127.0.0.1:0".to_string();
    world
}

#[given("a wireframe configuration binding to \"{bind}\"")]
fn given_bind(world: &BootstrapWorld, bind: String) { world.set_bind(bind); }

#[when("I bootstrap the wireframe server")]
fn when_bootstrap(world: &BootstrapWorld) { world.bootstrap(); }

#[then("bootstrap succeeds")]
fn then_success(world: &BootstrapWorld) {
    let outcome_ref = world.outcome.borrow();
    let Some(outcome) = outcome_ref.as_ref() else {
        panic!("bootstrap not executed");
    };
    assert_step_ok!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
}

#[then("the resolved bind address is \"{bind}\"")]
fn then_matches_bind(world: &BootstrapWorld, bind: String) {
    let outcome_ref = world.outcome.borrow();
    let Some(outcome) = outcome_ref.as_ref() else {
        panic!("bootstrap not executed");
    };
    let bootstrap = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
    assert_eq!(bootstrap.bind_addr.to_string(), bind);
}

#[then("bootstrap fails with message \"{message}\"")]
fn then_failure(world: &BootstrapWorld, message: String) {
    let outcome_ref = world.outcome.borrow();
    let Some(outcome) = outcome_ref.as_ref() else {
        panic!("bootstrap not executed");
    };
    let text = assert_step_err!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
    assert!(
        text.contains(&message),
        "expected '{text}' to contain '{message}'"
    );
}

#[scenario(path = "tests/features/wireframe_server.feature", index = 0)]
fn accepts_bind(world: BootstrapWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_server.feature", index = 1)]
fn rejects_bind(world: BootstrapWorld) { let _ = world; }
