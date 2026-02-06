//! Behavioural tests for outbound messaging adapters.

use std::{cell::RefCell, sync::Arc};

use mxd::{
    server::outbound::{OutboundError, OutboundMessaging, OutboundPriority, OutboundTarget},
    transaction::{FrameHeader, Transaction, parse_transaction},
    wireframe::outbound::{
        WireframeOutboundConnection,
        WireframeOutboundMessaging,
        WireframeOutboundRegistry,
    },
};
use rstest::fixture;
use rstest_bdd::assert_step_ok;
use rstest_bdd_macros::{given, scenarios, then, when};
use wireframe::push::{PushPriority, PushQueues};

struct OutboundWorld {
    connection: Arc<WireframeOutboundConnection>,
    messaging: WireframeOutboundMessaging,
    queues: RefCell<Option<PushQueues<Vec<u8>>>>,
    result: RefCell<Option<Result<(), OutboundError>>>,
}

impl OutboundWorld {
    fn new() -> Self {
        let registry = Arc::new(WireframeOutboundRegistry::default());
        let id = registry.allocate_id();
        let connection = Arc::new(WireframeOutboundConnection::new(id, registry));
        let messaging = WireframeOutboundMessaging::new(Arc::clone(&connection));
        Self {
            connection,
            messaging,
            queues: RefCell::new(None),
            result: RefCell::new(None),
        }
    }

    const fn message() -> Transaction {
        Transaction {
            header: FrameHeader {
                flags: 0,
                is_reply: 1,
                ty: 500,
                id: 42,
                error: 0,
                total_size: 0,
                data_size: 0,
            },
            payload: Vec::new(),
        }
    }
}

#[fixture]
fn world() -> OutboundWorld {
    let world = OutboundWorld::new();
    debug_assert!(
        world.queues.borrow().is_none(),
        "world starts without queues"
    );
    world
}

#[given("a wireframe outbound messenger with a registered connection")]
fn given_registered(world: &OutboundWorld) {
    let (queues, handle) = PushQueues::<Vec<u8>>::builder()
        .high_capacity(1)
        .low_capacity(1)
        .build()
        .unwrap_or_else(|err| panic!("push queues: {err}"));
    world.connection.register_handle(&handle);
    world.queues.replace(Some(queues));
}

#[given("a wireframe outbound messenger without a registered connection")]
fn given_unregistered(world: &OutboundWorld) { world.queues.replace(None); }

#[when("I push a low priority message to the current connection")]
async fn when_push_low(world: &OutboundWorld) {
    let result = world
        .messaging
        .push(
            OutboundTarget::Current,
            OutboundWorld::message(),
            OutboundPriority::Low,
        )
        .await;
    world.result.replace(Some(result));
}

#[then("the outbound push succeeds")]
fn then_push_succeeds(world: &OutboundWorld) {
    let result_ref = world.result.borrow();
    let Some(result) = result_ref.as_ref() else {
        panic!("no push result recorded");
    };
    assert_step_ok!(result.as_ref().map_err(ToString::to_string));
}

#[then("the outbound push fails with \"{message}\"")]
fn then_push_fails(world: &OutboundWorld, message: String) {
    let result_ref = world.result.borrow();
    let Some(result) = result_ref.as_ref() else {
        panic!("no push result recorded");
    };
    let Err(err) = result.as_ref() else {
        panic!("expected error");
    };
    assert_eq!(err.to_string(), message);
}

#[then("the low priority queue receives the message")]
async fn then_queue_receives(world: &OutboundWorld) {
    let mut queues = {
        let mut queues_slot = world.queues.borrow_mut();
        queues_slot
            .take()
            .unwrap_or_else(|| panic!("queues not initialised"))
    };
    let queued = queues.recv().await;
    world.queues.borrow_mut().replace(queues);
    let Some((priority, frame)) = queued else {
        panic!("frame queued");
    };
    assert_eq!(priority, PushPriority::Low);
    let parsed = parse_transaction(&frame).unwrap_or_else(|err| {
        panic!("parse transaction: {err}");
    });
    assert_eq!(parsed, OutboundWorld::message());
}

scenarios!(
    "tests/features/outbound_messaging.feature",
    runtime = "tokio-current-thread",
    fixtures = [world: OutboundWorld]
);
