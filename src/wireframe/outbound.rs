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
    ConnectionId,
    SessionRegistry,
    push::{PushError, PushHandle},
};

use crate::{
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
    handle: OnceLock<PushHandle<Vec<u8>>>,
}

impl WireframeOutboundConnection {
    /// Create a new outbound connection state.
    #[must_use]
    pub const fn new(id: OutboundConnectionId, registry: Arc<WireframeOutboundRegistry>) -> Self {
        Self {
            id,
            registry,
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
        }
        self.registry.insert(self.id, handle);
    }

    fn handle(&self) -> Option<PushHandle<Vec<u8>>> { self.handle.get().cloned() }

    fn registry(&self) -> &WireframeOutboundRegistry { &self.registry }
}

impl Drop for WireframeOutboundConnection {
    fn drop(&mut self) { self.registry.remove(self.id); }
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
    use rstest::{fixture, rstest};
    use tokio::runtime::Runtime;
    use wireframe::push::PushQueues;

    use super::*;
    use crate::transaction::FrameHeader;

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
        let connection = Arc::new(WireframeOutboundConnection::new(id, registry));
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
        let connection = Arc::new(WireframeOutboundConnection::new(id, Arc::clone(&registry)));
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
}
