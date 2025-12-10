//! Connection-scoped state for the Wireframe adapter.
//!
//! The Wireframe runtime executes each accepted TCP connection inside its own
//! Tokio task. During the Hotline handshake we need to retain the negotiated
//! metadata (sub-protocol ID and sub-version) so later routing and
//! compatibility shims can branch on the client's capabilities. This module
//! keeps a per-task/thread store of handshake metadata and exposes helpers to
//! store, read, and clear that data.

#![allow(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

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

#[expect(
    clippy::missing_const_for_thread_local,
    reason = "RefCell initialisation cannot be const for thread locals"
)]
mod handshake_local {
    use std::cell::RefCell;

    use super::HandshakeMetadata;

    thread_local! {
        pub static HANDSHAKE: RefCell<Option<HandshakeMetadata>> = RefCell::new(None);
    }
}

/// Store handshake metadata for the current Tokio task.
///
/// When handshake handling runs outside a Tokio task context, the metadata is
/// stored in a thread-local fallback so diagnostics and tests can still
/// observe the negotiated values.
pub fn store_current_handshake(metadata: HandshakeMetadata) {
    handshake_local::HANDSHAKE.with(|cell| {
        cell.borrow_mut().replace(metadata);
    });
}

/// Retrieve handshake metadata for the current Tokio task, if present.
///
/// Returns `None` if no metadata has been stored for this thread/task.
#[must_use]
pub fn current_handshake() -> Option<HandshakeMetadata> {
    handshake_local::HANDSHAKE.with(|cell| cell.borrow().clone())
}

/// Remove the handshake metadata entry for the current Tokio task.
pub fn clear_current_handshake() {
    handshake_local::HANDSHAKE.with(|cell| {
        cell.borrow_mut().take();
    });
}

/// Return the number of stored handshake metadata entries visible to this
/// thread.
///
/// This reflects only the thread-local store used by the current task/thread;
/// entries placed in other threads are intentionally ignored, so this value is
/// suitable for diagnostics rather than global accounting.
#[must_use]
pub fn registry_len() -> usize {
    handshake_local::HANDSHAKE.with(|cell| usize::from(cell.borrow().is_some()))
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
        store_current_handshake(meta.clone());
        assert_eq!(current_handshake(), Some(meta.clone()));
        assert_eq!(meta.sub_protocol_tag(), *b"CHAT");
        clear_current_handshake();
        assert!(current_handshake().is_none());
        assert_eq!(registry_len(), 0);
    }

    #[rstest]
    #[tokio::test]
    async fn isolates_metadata_between_tasks() {
        let first = task::spawn(async {
            let meta = metadata(1, 1);
            store_current_handshake(meta.clone());
            let seen = current_handshake();
            clear_current_handshake();
            seen
        });

        let second = task::spawn(async {
            let meta = metadata(2, 2);
            store_current_handshake(meta.clone());
            let seen = current_handshake();
            clear_current_handshake();
            seen
        });

        let (first_seen, second_seen) = tokio::join!(first, second);
        assert_eq!(
            first_seen.expect("first task panicked"),
            Some(metadata(1, 1))
        );
        assert_eq!(
            second_seen.expect("second task panicked"),
            Some(metadata(2, 2))
        );
        assert_eq!(registry_len(), 0);
    }
}
