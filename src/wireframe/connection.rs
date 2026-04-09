//! Connection-scoped state for the Wireframe adapter.
//!
//! The Wireframe runtime executes each accepted TCP connection inside its own
//! Tokio task. During the Hotline handshake we need to retain the negotiated
//! metadata (sub-protocol ID and sub-version) so later routing and
//! compatibility shims can branch on the client's capabilities. This module
//! keeps handshake metadata and peer information in Tokio task-local storage so
//! connection setup can seed the app factory with the negotiated context even
//! when Tokio migrates the task across worker threads. Wireframe's synchronous
//! app factory runs after the handshake future resolves, so this module also
//! mirrors the scoped value in a task-ID keyed registry for that hand-off.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use std::{
    cell::RefCell,
    collections::HashMap,
    net::SocketAddr,
    sync::{Mutex, OnceLock},
};

use tokio::task::{self, Id};

use crate::protocol::{Handshake, VERSION};

/// Handshake parameters captured from the Hotline preamble.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeMetadata {
    /// Application-specific sub-protocol identifier.
    pub sub_protocol: u32,
    /// Protocol version negotiated during the handshake.
    pub version: u16,
    /// Application-defined sub-version negotiated during the handshake.
    pub sub_version: u16,
}

impl Default for HandshakeMetadata {
    fn default() -> Self {
        Self {
            sub_protocol: 0,
            version: VERSION,
            sub_version: 0,
        }
    }
}

impl HandshakeMetadata {
    /// Return the four-byte sub-protocol tag in network byte order.
    #[must_use]
    pub const fn sub_protocol_tag(&self) -> [u8; 4] { self.sub_protocol.to_be_bytes() }
}

impl From<Handshake> for HandshakeMetadata {
    fn from(value: Handshake) -> Self {
        Self {
            sub_protocol: value.sub_protocol,
            version: value.version,
            sub_version: value.sub_version,
        }
    }
}

impl From<&Handshake> for HandshakeMetadata {
    fn from(value: &Handshake) -> Self {
        Self {
            sub_protocol: value.sub_protocol,
            version: value.version,
            sub_version: value.sub_version,
        }
    }
}

/// Connection-scoped handshake and peer metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionContext {
    handshake: HandshakeMetadata,
    peer: Option<SocketAddr>,
}

impl ConnectionContext {
    /// Create a connection context from handshake metadata.
    #[must_use]
    pub const fn new(handshake: HandshakeMetadata) -> Self {
        Self {
            handshake,
            peer: None,
        }
    }

    /// Return the handshake metadata.
    #[must_use]
    pub const fn handshake(&self) -> &HandshakeMetadata { &self.handshake }

    /// Return the peer address, if known.
    #[must_use]
    pub const fn peer(&self) -> Option<SocketAddr> { self.peer }

    /// Attach a peer address to the context.
    #[must_use]
    pub const fn with_peer(mut self, peer: SocketAddr) -> Self {
        self.peer = Some(peer);
        self
    }

    /// Consume the context and return the handshake metadata and peer address.
    #[must_use]
    pub const fn into_parts(self) -> (HandshakeMetadata, Option<SocketAddr>) {
        (self.handshake, self.peer)
    }
}

tokio::task_local! {
    static CONNECTION_CONTEXT: RefCell<Option<ConnectionContext>>;
}

fn registry() -> &'static Mutex<HashMap<Id, ConnectionContext>> {
    static CONNECTION_CONTEXT_REGISTRY: OnceLock<Mutex<HashMap<Id, ConnectionContext>>> =
        OnceLock::new();
    CONNECTION_CONTEXT_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn current_task_id() -> Option<Id> { task::try_id() }

fn store_registry_context(context: &ConnectionContext) {
    if let Some(task_id) = current_task_id() {
        registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(task_id, context.clone());
    }
}

fn registry_context() -> Option<ConnectionContext> {
    let task_id = current_task_id()?;
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&task_id)
        .cloned()
}

fn take_registry_context() -> Option<ConnectionContext> {
    let task_id = current_task_id()?;
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&task_id)
}

/// Scope connection context metadata for the current Tokio task.
///
/// The returned future installs a task-local slot before the first poll and
/// keeps it available for the lifetime of the scoped future.
pub fn scope_current_context<F>(
    context: Option<ConnectionContext>,
    future: F,
) -> impl std::future::Future<Output = F::Output>
where
    F: std::future::Future,
{
    CONNECTION_CONTEXT.scope(RefCell::new(context), future)
}

/// Store connection context metadata for the current Tokio task.
///
/// When the current code is already running inside a scoped connection task,
/// the task-local slot is updated immediately. The value is also mirrored into
/// a task-ID keyed registry so synchronous app-factory code in the same Tokio
/// task can still consume it after the handshake future completes.
pub fn store_current_context(context: ConnectionContext) {
    store_registry_context(&context);
    match CONNECTION_CONTEXT.try_with(|cell| {
        cell.borrow_mut().replace(context);
    }) {
        Ok(()) | Err(_) => {}
    }
}

/// Retrieve connection context metadata for the current Tokio task, if present.
///
/// Returns `None` if no metadata has been stored for this task.
#[must_use]
pub fn current_context() -> Option<ConnectionContext> {
    CONNECTION_CONTEXT
        .try_with(|cell| cell.borrow().clone())
        .ok()
        .flatten()
        .or_else(registry_context)
}

/// Take the connection context entry for the current Tokio task.
#[must_use]
pub fn take_current_context() -> Option<ConnectionContext> {
    let from_task_local = CONNECTION_CONTEXT
        .try_with(|cell| cell.borrow_mut().take())
        .ok()
        .flatten();
    let taken = from_task_local.or_else(take_registry_context);
    if taken.is_some() {
        let _ = take_registry_context();
    }
    taken
}

/// Return the number of stored connection context entries visible to this task.
#[must_use]
pub fn registry_len() -> usize { usize::from(current_context().is_some()) }

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tokio::{runtime::Builder, sync::Barrier, task};

    use super::*;

    fn metadata(sub_protocol: u32, sub_version: u16) -> HandshakeMetadata {
        HandshakeMetadata {
            sub_protocol,
            version: VERSION,
            sub_version,
        }
    }

    #[rstest]
    #[tokio::test]
    async fn stores_and_reads_metadata_in_task() {
        let meta = metadata(u32::from_be_bytes(*b"CHAT"), 7);
        let context = ConnectionContext::new(meta.clone());
        scope_current_context(None, async {
            store_current_context(context.clone());
            assert_eq!(current_context(), Some(context.clone()));
            assert_eq!(context.handshake().sub_protocol_tag(), *b"CHAT");
            let _ = take_current_context();
            assert!(current_context().is_none());
            assert_eq!(registry_len(), 0);
        })
        .await;
    }

    #[rstest]
    #[tokio::test]
    async fn isolates_metadata_between_tasks() {
        let first = task::spawn(async {
            let meta = metadata(1, 1);
            let context = ConnectionContext::new(meta.clone());
            scope_current_context(None, async move {
                store_current_context(context.clone());
                let seen = current_context();
                let _ = take_current_context();
                seen
            })
            .await
        });

        let second = task::spawn(async {
            let meta = metadata(2, 2);
            let context = ConnectionContext::new(meta.clone());
            scope_current_context(None, async move {
                store_current_context(context.clone());
                let seen = current_context();
                let _ = take_current_context();
                seen
            })
            .await
        });

        let (first_seen, second_seen) = tokio::join!(first, second);
        assert_eq!(
            first_seen
                .expect("first task panicked")
                .map(|context| context.handshake),
            Some(metadata(1, 1))
        );
        assert_eq!(
            second_seen
                .expect("second task panicked")
                .map(|context| context.handshake),
            Some(metadata(2, 2))
        );
        assert_eq!(registry_len(), 0);
    }

    #[rstest]
    fn preserves_context_across_await_on_multi_worker_runtime() {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build multi-worker runtime");
        let barrier = std::sync::Arc::new(Barrier::new(2));

        runtime.block_on(async {
            let meta = metadata(u32::from_be_bytes(*b"CHAT"), 9);
            let context = ConnectionContext::new(meta.clone());
            let task_barrier = barrier.clone();

            let seen = task::spawn(async move {
                scope_current_context(Some(context.clone()), async move {
                    task_barrier.wait().await;
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    take_current_context()
                })
                .await
            });

            barrier.wait().await;
            assert_eq!(
                seen.await.expect("context task panicked"),
                Some(ConnectionContext::new(meta))
            );
        });
    }
}
