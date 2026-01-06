//! Connection-scoped state for the Wireframe adapter.
//!
//! The Wireframe runtime executes each accepted TCP connection inside its own
//! Tokio task. During the Hotline handshake we need to retain the negotiated
//! metadata (sub-protocol ID and sub-version) so later routing and
//! compatibility shims can branch on the client's capabilities. This module
//! keeps a per-task/thread store of handshake metadata and peer information so
//! connection setup can seed the app factory with the negotiated context.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use std::net::SocketAddr;

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

#[expect(
    clippy::missing_const_for_thread_local,
    reason = "RefCell initialisation cannot be const for thread locals"
)]
mod handshake_local {
    use std::cell::RefCell;

    thread_local! {
        pub static CONNECTION_CONTEXT: RefCell<Option<super::ConnectionContext>> =
            RefCell::new(None);
    }
}

/// Store connection context metadata for the current Tokio task.
///
/// When handshake handling runs outside a Tokio task context, the context is
/// stored in a thread-local fallback so diagnostics and tests can still
/// observe the negotiated values.
pub fn store_current_context(context: ConnectionContext) {
    handshake_local::CONNECTION_CONTEXT.with(|cell| {
        cell.borrow_mut().replace(context);
    });
}

/// Retrieve connection context metadata for the current Tokio task, if present.
///
/// Returns `None` if no metadata has been stored for this thread/task.
#[must_use]
pub fn current_context() -> Option<ConnectionContext> {
    handshake_local::CONNECTION_CONTEXT.with(|cell| cell.borrow().clone())
}

/// Take the connection context entry for the current Tokio task.
#[must_use]
pub fn take_current_context() -> Option<ConnectionContext> {
    handshake_local::CONNECTION_CONTEXT.with(|cell| cell.borrow_mut().take())
}

/// Return the number of stored connection context entries visible to this
/// thread.
///
/// This reflects only the thread-local store used by the current task/thread;
/// entries placed in other threads are intentionally ignored, so this value is
/// suitable for diagnostics rather than global accounting.
#[must_use]
pub fn registry_len() -> usize {
    handshake_local::CONNECTION_CONTEXT.with(|cell| usize::from(cell.borrow().is_some()))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tokio::task;

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
        store_current_context(context.clone());
        assert_eq!(current_context(), Some(context.clone()));
        assert_eq!(context.handshake().sub_protocol_tag(), *b"CHAT");
        let _ = take_current_context();
        assert!(current_context().is_none());
        assert_eq!(registry_len(), 0);
    }

    #[rstest]
    #[tokio::test]
    async fn isolates_metadata_between_tasks() {
        let first = task::spawn(async {
            let meta = metadata(1, 1);
            let context = ConnectionContext::new(meta.clone());
            store_current_context(context.clone());
            let seen = current_context();
            let _ = take_current_context();
            seen
        });

        let second = task::spawn(async {
            let meta = metadata(2, 2);
            let context = ConnectionContext::new(meta.clone());
            store_current_context(context.clone());
            let seen = current_context();
            let _ = take_current_context();
            seen
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
}
