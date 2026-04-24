//! Wireframe outbound messaging adapter.
//!
//! This module implements the outbound messaging trait for the wireframe
//! transport, mapping domain transactions to wireframe push queues.

use std::sync::{
    Arc,
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

use async_trait::async_trait;
use tracing::warn;
use wireframe::{
    push::{PushError, PushHandle},
    session::{ConnectionId, SessionRegistry},
};

use crate::{
    presence::{PresenceRegistry, build_notify_delete_user},
    server::outbound::{
        OutboundConnectionId,
        OutboundError,
        OutboundMessaging,
        OutboundPriority,
        OutboundTarget,
    },
    transaction::Transaction,
};

/// Shared registry for mapping outbound connection identifiers to push handles.
pub struct WireframeOutboundRegistry {
    next_id: AtomicU64,
    sessions: SessionRegistry<Vec<u8>>,
}

impl Default for WireframeOutboundRegistry {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            sessions: SessionRegistry::default(),
        }
    }
}

impl WireframeOutboundRegistry {
    /// Allocate a new outbound connection identifier.
    #[must_use]
    pub fn allocate_id(&self) -> OutboundConnectionId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        OutboundConnectionId::new(id)
    }

    fn insert(&self, id: OutboundConnectionId, handle: &PushHandle<Vec<u8>>) {
        self.sessions.insert(ConnectionId::new(id.as_u64()), handle);
    }

    fn remove(&self, id: OutboundConnectionId) {
        self.sessions.remove(&ConnectionId::new(id.as_u64()));
    }

    fn handle_for(&self, id: OutboundConnectionId) -> Option<PushHandle<Vec<u8>>> {
        self.sessions.get(&ConnectionId::new(id.as_u64()))
    }

    fn active_handles(&self) -> Vec<PushHandle<Vec<u8>>> {
        self.sessions
            .active_handles()
            .into_iter()
            .map(|(_, handle)| handle)
            .collect()
    }
}

/// Per-connection outbound state for wireframe messaging.
pub struct WireframeOutboundConnection {
    id: OutboundConnectionId,
    registry: Arc<WireframeOutboundRegistry>,
    presence: Arc<PresenceRegistry>,
    handle: OnceLock<PushHandle<Vec<u8>>>,
}

impl WireframeOutboundConnection {
    /// Create a new outbound connection state.
    #[must_use]
    pub const fn new(
        id: OutboundConnectionId,
        registry: Arc<WireframeOutboundRegistry>,
        presence: Arc<PresenceRegistry>,
    ) -> Self {
        Self {
            id,
            registry,
            presence,
            handle: OnceLock::new(),
        }
    }

    /// Return the outbound connection identifier.
    #[must_use]
    pub const fn id(&self) -> OutboundConnectionId { self.id }

    /// Register the push handle for this connection.
    pub fn register_handle(&self, handle: &PushHandle<Vec<u8>>) {
        if self.handle.set(handle.clone()).is_err() {
            warn!("outbound push handle already registered");
            return;
        }
        self.registry.insert(self.id, handle);
    }

    fn handle(&self) -> Option<PushHandle<Vec<u8>>> { self.handle.get().cloned() }

    fn registry(&self) -> &WireframeOutboundRegistry { &self.registry }

    fn take_disconnect_notification(&self) -> Option<(i32, Vec<OutboundConnectionId>, Vec<u8>)> {
        let removal_result = self.presence.remove(self.id);
        self.registry.remove(self.id);
        let removal = removal_result?;
        let user_id = removal.departed.user_id;
        let encoded_message = build_notify_delete_user(user_id).map_err(|error| {
            warn!(?error, user_id, "failed to encode notify-delete-user");
        });
        let message = encoded_message.ok()?;
        Some((user_id, removal.remaining_peer_ids, message.to_bytes()))
    }

    fn spawn_disconnect_notification(
        registry: Arc<WireframeOutboundRegistry>,
        user_id: i32,
        peer_ids: Vec<OutboundConnectionId>,
        bytes: Vec<u8>,
    ) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            warn!(user_id, "no runtime available for disconnect notification");
            return;
        };
        runtime.spawn(async move {
            push_disconnect_notifications(registry, peer_ids, bytes).await;
        });
    }
}

impl Drop for WireframeOutboundConnection {
    fn drop(&mut self) {
        let Some((user_id, peer_ids, bytes)) = self.take_disconnect_notification() else {
            return;
        };
        let registry = Arc::clone(&self.registry);
        Self::spawn_disconnect_notification(registry, user_id, peer_ids, bytes);
    }
}

async fn push_disconnect_notifications(
    registry: Arc<WireframeOutboundRegistry>,
    peer_ids: Vec<OutboundConnectionId>,
    bytes: Vec<u8>,
) {
    for connection_id in peer_ids {
        push_disconnect_notification(&registry, connection_id, &bytes).await;
    }
}

async fn push_disconnect_notification(
    registry: &WireframeOutboundRegistry,
    connection_id: OutboundConnectionId,
    bytes: &[u8],
) {
    let Some(handle) = registry.handle_for(connection_id) else {
        return;
    };
    if let Err(error) = handle.push_high_priority(bytes.to_vec()).await {
        warn!(
            ?error,
            target = connection_id.as_u64(),
            "disconnect push failed"
        );
    }
}

/// Wireframe implementation of outbound messaging.
#[derive(Clone)]
pub struct WireframeOutboundMessaging {
    connection: Arc<WireframeOutboundConnection>,
}

impl WireframeOutboundMessaging {
    /// Create a new wireframe outbound messaging adapter.
    #[must_use]
    pub const fn new(connection: Arc<WireframeOutboundConnection>) -> Self { Self { connection } }

    /// Return the outbound identifier for the current connection.
    #[must_use]
    pub fn connection_id(&self) -> OutboundConnectionId { self.connection.id() }

    async fn push_bytes(
        handle: &PushHandle<Vec<u8>>,
        bytes: Vec<u8>,
        priority: OutboundPriority,
    ) -> Result<(), OutboundError> {
        let result = match priority {
            OutboundPriority::High => handle.push_high_priority(bytes).await,
            OutboundPriority::Low => handle.push_low_priority(bytes).await,
        };
        result.map_err(map_push_error)
    }
}

#[async_trait]
impl OutboundMessaging for WireframeOutboundMessaging {
    async fn push(
        &self,
        target: OutboundTarget,
        message: Transaction,
        priority: OutboundPriority,
    ) -> Result<(), OutboundError> {
        let maybe_handle = match target {
            OutboundTarget::Current => self.connection.handle(),
            OutboundTarget::Connection(id) => self.connection.registry().handle_for(id),
        };
        let Some(handle) = maybe_handle else {
            return Err(OutboundError::TargetUnavailable);
        };
        Self::push_bytes(&handle, message.to_bytes(), priority).await
    }

    async fn broadcast(
        &self,
        message: Transaction,
        priority: OutboundPriority,
    ) -> Result<(), OutboundError> {
        let handles = self.connection.registry().active_handles();
        if handles.is_empty() {
            return Err(OutboundError::TargetUnavailable);
        }
        let bytes = message.to_bytes();
        for handle in handles {
            Self::push_bytes(&handle, bytes.clone(), priority).await?;
        }
        Ok(())
    }
}

const fn map_push_error(error: PushError) -> OutboundError {
    match error {
        PushError::QueueFull => OutboundError::QueueFull,
        _ => OutboundError::QueueClosed,
    }
}

#[cfg(test)]
mod tests {
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
        let departing = Arc::new(WireframeOutboundConnection::new(
            departing_id,
            Arc::clone(&registry),
            Arc::clone(&presence),
        ));
        let remaining_id = registry.allocate_id();
        let remaining = Arc::new(WireframeOutboundConnection::new(
            remaining_id,
            Arc::clone(&registry),
            Arc::clone(&presence),
        ));

        let (mut queues, handle) = PushQueues::<Vec<u8>>::builder()
            .high_capacity(1)
            .low_capacity(1)
            .build()
            .expect("push queues");
        remaining.register_handle(&handle);

        let _ = presence.upsert(PresenceSnapshot {
            connection_id: departing_id,
            user_id: 7,
            display_name: "alice".to_owned(),
            icon_id: 0,
            status_flags: 0,
        });
        let _ = presence.upsert(PresenceSnapshot {
            connection_id: remaining_id,
            user_id: 8,
            display_name: "bob".to_owned(),
            icon_id: 0,
            status_flags: 0,
        });

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
            assert_eq!(user_id, 7);
        });

        drop(remaining);
    }
}
