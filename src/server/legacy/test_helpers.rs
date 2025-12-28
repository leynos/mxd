//! Shared helper utilities for exercising legacy server logic in tests.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use crate::{db::DbPool, protocol};

/// Re-export the shared `dummy_pool` helper from `wireframe::test_helpers`.
///
/// This avoids duplication across test modules; all tests share the same pool
/// configuration for consistency.
pub(super) fn dummy_pool() -> DbPool { crate::wireframe::test_helpers::dummy_pool() }

pub(super) fn handshake_frame() -> [u8; protocol::HANDSHAKE_LEN] {
    let mut buf = [0u8; protocol::HANDSHAKE_LEN];
    buf[0..4].copy_from_slice(protocol::PROTOCOL_ID);
    buf[8..10].copy_from_slice(&protocol::VERSION.to_be_bytes());
    buf
}
