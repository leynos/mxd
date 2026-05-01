//! Unit tests for wireframe outbound messaging.

use std::sync::Arc;

use rstest::{fixture, rstest};
use tokio::runtime::Runtime;
use wireframe::push::PushQueues;

use super::*;
use crate::{
    field_id::FieldId,
    presence::{PresenceRegistry, PresenceSnapshot},
    transaction::{FrameHeader, decode_params},
};

#[fixture]
fn reply() -> Transaction {
    Transaction {
        header: FrameHeader {
            flags: 0,
            is_reply: 1,
            ty: 1,
            id: 7,
            error: 0,
            total_size: 0,
            data_size: 0,
        },
        payload: Vec::new(),
    }
}

#[rstest]
fn push_to_current_requires_handle(reply: Transaction) {
    let registry = Arc::new(WireframeOutboundRegistry::default());
    let id = registry.allocate_id();
    let connection = Arc::new(WireframeOutboundConnection::new(
        id,
        registry,
        Arc::new(PresenceRegistry::default()),
    ));
    let messaging = WireframeOutboundMessaging::new(connection);
    let rt = Runtime::new().expect("runtime");

    let err = rt
        .block_on(messaging.push(OutboundTarget::Current, reply, OutboundPriority::High))
        .expect_err("missing handle");

    assert_eq!(err, OutboundError::TargetUnavailable);
}

#[rstest]
fn push_to_current_enqueues_frame(reply: Transaction) {
    let registry = Arc::new(WireframeOutboundRegistry::default());
    let id = registry.allocate_id();
    let connection = Arc::new(WireframeOutboundConnection::new(
        id,
        Arc::clone(&registry),
        Arc::new(PresenceRegistry::default()),
    ));
    let messaging = WireframeOutboundMessaging::new(Arc::clone(&connection));
    let rt = Runtime::new().expect("runtime");

    let (mut queues, handle) = PushQueues::<Vec<u8>>::builder()
        .high_capacity(1)
        .low_capacity(1)
        .build()
        .expect("push queues");
    connection.register_handle(&handle);

    rt.block_on(messaging.push(
        OutboundTarget::Current,
        reply.clone(),
        OutboundPriority::High,
    ))
    .expect("push ok");

    let (priority, frame) = rt
        .block_on(async { queues.recv().await })
        .expect("frame queued");
    assert_eq!(priority, wireframe::push::PushPriority::High);
    let parsed = crate::transaction::parse_transaction(&frame).expect("parse reply");
    assert_eq!(parsed, reply);
}

#[rstest]
fn dropping_connection_broadcasts_notify_delete_user() {
    let rt = Runtime::new().expect("runtime");
    let registry = Arc::new(WireframeOutboundRegistry::default());
    let presence = Arc::new(PresenceRegistry::default());

    let departing_id = registry.allocate_id();
    let departing = Arc::new(WireframeOutboundConnection::new_with_runtime_handle(
        departing_id,
        Arc::clone(&registry),
        Arc::clone(&presence),
        Some(rt.handle().clone()),
    ));
    let remaining_id = registry.allocate_id();
    let remaining = Arc::new(WireframeOutboundConnection::new_with_runtime_handle(
        remaining_id,
        Arc::clone(&registry),
        Arc::clone(&presence),
        Some(rt.handle().clone()),
    ));

    let (mut queues, handle) = PushQueues::<Vec<u8>>::builder()
        .high_capacity(1)
        .low_capacity(1)
        .build()
        .expect("push queues");
    remaining.register_handle(&handle);

    let departing_presence = presence
        .upsert(PresenceSnapshot {
            connection_id: departing_id,
            user_id: 7,
            display_name: "alice".to_owned(),
            icon_id: 0,
            status_flags: 0,
        })
        .expect("insert departing presence");
    presence
        .upsert(PresenceSnapshot {
            connection_id: remaining_id,
            user_id: 8,
            display_name: "bob".to_owned(),
            icon_id: 0,
            status_flags: 0,
        })
        .expect("insert remaining presence");

    rt.block_on(async {
        drop(departing);
        tokio::task::yield_now().await;
        let (_, frame) = queues.recv().await.expect("queued disconnect notification");
        let parsed = crate::transaction::parse_transaction(&frame).expect("parse transaction");
        assert_eq!(parsed.header.ty, 302);
        let params = decode_params(&parsed.payload).expect("decode params");
        let user_id = params
            .iter()
            .find(|(field_id, _)| *field_id == FieldId::UserId)
            .map(|(_, bytes)| i32::from_be_bytes(bytes.as_slice().try_into().expect("user id")))
            .expect("user id field");
        assert_eq!(user_id, departing_presence.snapshot.user_id);
    });

    drop(remaining);
}
