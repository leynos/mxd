//! Connection-scoped state for the Wireframe adapter.
//!
//! The Wireframe runtime executes each accepted TCP connection inside its own
//! Tokio task. During the Hotline handshake we need to retain the negotiated
//! metadata (sub-protocol ID and sub-version) so later routing and
//! compatibility shims can branch on the client’s capabilities. This module
//! keeps a small registry of handshake metadata keyed by the current task ID
//! and exposes helpers to store, read, and clear that data.

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{OnceLock, RwLock},
};

use tokio::task;

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
    pub fn sub_protocol_tag(&self) -> [u8; 4] { self.sub_protocol.to_be_bytes() }
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

fn registry() -> &'static RwLock<HashMap<task::Id, HandshakeMetadata>> {
    static REGISTRY: OnceLock<RwLock<HashMap<task::Id, HandshakeMetadata>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

thread_local! {
    static THREAD_HANDSHAKE: RefCell<Option<HandshakeMetadata>> = const { RefCell::new(None) };
}

/// Store handshake metadata for the current Tokio task.
///
/// If the task ID cannot be determined (which should not occur for tasks
/// spawned by the Wireframe runtime), the function logs a warning and leaves
/// the registry unchanged.
///
/// # Panics
///
/// Panics if the handshake registry lock has been poisoned.
pub fn store_current_handshake(metadata: HandshakeMetadata) {
    if let Some(id) = task::try_id() {
        let mut guard = registry().write().expect("handshake registry poisoned");
        guard.insert(id, metadata);
    } else {
        tracing::debug!("storing handshake metadata in thread-local fallback");
        THREAD_HANDSHAKE.with(|cell| {
            cell.borrow_mut().replace(metadata);
        });
    }
}

/// Retrieve handshake metadata for the current Tokio task, if present.
#[must_use]
pub fn current_handshake() -> Option<HandshakeMetadata> {
    if let Some(id) = task::try_id() {
        let guard = registry().read().ok()?;
        guard.get(&id).cloned()
    } else {
        THREAD_HANDSHAKE.with(|cell| cell.borrow().clone())
    }
}

/// Remove the handshake metadata entry for the current Tokio task.
pub fn clear_current_handshake() {
    if let Some(id) = task::try_id() {
        if let Ok(mut guard) = registry().write() {
            guard.remove(&id);
        }
    } else {
        THREAD_HANDSHAKE.with(|cell| {
            cell.borrow_mut().take();
        });
    }
}

/// Return the number of stored handshake metadata entries.
///
/// # Panics
///
/// Panics if the registry lock has been poisoned.
#[must_use]
pub fn registry_len() -> usize {
    let task_entries = registry()
        .read()
        .expect("handshake registry poisoned")
        .len();
    let thread_entry = THREAD_HANDSHAKE.with(|cell| usize::from(cell.borrow().is_some()));
    task_entries + thread_entry
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
