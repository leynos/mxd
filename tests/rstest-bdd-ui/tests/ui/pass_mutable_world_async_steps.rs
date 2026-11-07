//! Compile-pass fixture for the trybuild suite in `tests/ui.rs`.
//! It pins the mutable-world async pattern the handshake scenarios rely on: a
//! borrowed world fixture, a synchronous `Given`, an async `When` that borrows
//! the world mutably, a synchronous `Then`, and a named scenario driven by an
//! external current-thread Tokio test.

use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};

#[derive(Default)]
struct World {
    recorded: Option<u32>,
}

#[fixture]
fn world() -> World { World::default() }

#[given("a mutable world")]
fn given_world(world: &mut World) { world.recorded = None; }

#[when("the world records a value")]
async fn when_records(world: &mut World) { world.recorded = Some(7); }

#[then("the value is recorded")]
fn then_recorded(world: &World) { assert_eq!(world.recorded, Some(7)); }

#[scenario(
    path = "../../../../tests/rstest-bdd-ui/tests/ui/mutable_world.feature",
    name = "async mutable world",
)]
#[tokio::test(flavor = "current_thread")]
async fn async_mutable_world(_world: World) {}

fn main() {}
