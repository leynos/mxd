//! `WireframeProtocol` adapter for Hotline transaction routing.
//!
//! This module implements the [`WireframeProtocol`] trait from the wireframe
//! library, providing connection lifecycle callbacks that integrate with the
//! domain layer. The adapter maintains per-connection state and applies
//! protocol-specific hooks during the connection lifecycle.
//!
//! # Architecture
//!
//! The `HotlineProtocol` adapter follows hexagonal architecture principles:
//!
//! - **Inbound port**: Transaction routing is handled via `WireframeApp::route()` registrations in
//!   the server bootstrap, not within this trait.
//! - **Lifecycle hooks**: This trait provides connection setup, frame mutation, and error handling
//!   callbacks.
//! - **Domain isolation**: The adapter bridges wireframe types to domain types without leaking
//!   wireframe dependencies into domain code.
//!
//! # Usage
//!
//! Register the protocol adapter with the wireframe app:
//!
//! ```rust,ignore
//! use mxd::wireframe::protocol::HotlineProtocol;
//!
//! let protocol = HotlineProtocol::new(pool.clone(), argon2.clone());
//! let app = WireframeApp::default()
//!     .with_protocol(protocol);
//! ```
//!
//! # Note on Frame Type
//!
//! The wireframe library requires `Frame = Vec<u8>` and `ProtocolError = ()` for
//! the default `WireframeApp`. The actual Hotline framing (20-byte headers,
//! multi-fragment reassembly) is handled by the `HotlineTransaction` codec at
//! the preamble/connection level, while the protocol hooks operate on raw bytes.

use std::sync::Arc;

use argon2::Argon2;
use tracing::info;
use wireframe::{ConnectionContext, WireframeProtocol, push::PushHandle};

use crate::db::DbPool;

/// `WireframeProtocol` implementation for the Hotline protocol.
///
/// This adapter provides connection lifecycle hooks that integrate with the
/// MXD domain layer. It is registered with `WireframeApp::with_protocol()` and
/// receives callbacks during connection setup, frame transmission, and error
/// handling.
///
/// # Thread Safety
///
/// The protocol adapter is designed to be shared across connections. Per-
/// connection state is managed via [`ConnectionContext`] and app data, not
/// within this struct.
pub struct HotlineProtocol {
    pool: DbPool,
    argon2: Arc<Argon2<'static>>,
}

impl HotlineProtocol {
    /// Create a new Hotline protocol adapter.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool for transaction handlers.
    /// * `argon2` - Shared Argon2 instance for password hashing.
    #[must_use]
    pub const fn new(pool: DbPool, argon2: Arc<Argon2<'static>>) -> Self { Self { pool, argon2 } }

    /// Return a reference to the database pool.
    #[must_use]
    pub const fn pool(&self) -> &DbPool { &self.pool }

    /// Return a reference to the Argon2 instance.
    #[must_use]
    pub const fn argon2(&self) -> &Arc<Argon2<'static>> { &self.argon2 }
}

impl WireframeProtocol for HotlineProtocol {
    // The wireframe library's WireframeApp::with_protocol requires these exact types
    type Frame = Vec<u8>;
    type ProtocolError = ();

    fn on_connection_setup(&self, _handle: PushHandle<Self::Frame>, _ctx: &mut ConnectionContext) {
        // Connection setup is handled in the app factory via build_app(), which
        // creates ConnectionState with session, context, and handshake metadata.
        // The push handle could be stored for outbound messaging in future work.
        info!("hotline connection established");
    }

    fn before_send(&self, _frame: &mut Self::Frame, _ctx: &mut ConnectionContext) {
        // Future work: Apply compatibility shims based on handshake sub_version.
        // For example, XOR-encode text fields for legacy SynHX clients.
    }

    fn on_command_end(&self, _ctx: &mut ConnectionContext) {
        // Called when a request/response cycle completes. Currently a no-op.
    }

    fn handle_error(&self, _error: Self::ProtocolError, _ctx: &mut ConnectionContext) {
        // The unit type () provides no error information, so we just log a generic message.
        // Detailed error handling occurs in the route handlers before reaching this hook.
    }

    fn stream_end_frame(&self, _ctx: &mut ConnectionContext) -> Option<Self::Frame> {
        // Hotline protocol does not use explicit end-of-stream frames for
        // request/response cycles. Multi-packet streaming (e.g., file transfers)
        // will be handled separately.
        None
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use wireframe::ConnectionContext;

    use super::*;
    use crate::wireframe::test_helpers::dummy_pool;

    #[rstest]
    fn protocol_can_be_created() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());

        let protocol = HotlineProtocol::new(pool, argon2);

        assert!(Arc::strong_count(protocol.argon2()) >= 1);
    }

    #[rstest]
    fn protocol_shares_argon2_instance() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let argon2_clone = Arc::clone(&argon2);

        let protocol = HotlineProtocol::new(pool, argon2);

        assert!(Arc::ptr_eq(protocol.argon2(), &argon2_clone));
    }

    #[rstest]
    fn handle_error_does_not_panic() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let protocol = HotlineProtocol::new(pool, argon2);
        let mut ctx = ConnectionContext;

        // Should not panic - error type is () so no detailed error info
        protocol.handle_error((), &mut ctx);
    }

    #[rstest]
    fn stream_end_frame_returns_none() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let protocol = HotlineProtocol::new(pool, argon2);
        let mut ctx = ConnectionContext;

        let frame = protocol.stream_end_frame(&mut ctx);

        assert!(frame.is_none());
    }

    #[rstest]
    fn on_command_end_does_not_panic() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let protocol = HotlineProtocol::new(pool, argon2);
        let mut ctx = ConnectionContext;

        // Should not panic
        protocol.on_command_end(&mut ctx);
    }

    #[rstest]
    fn before_send_does_not_panic() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let protocol = HotlineProtocol::new(pool, argon2);
        let mut ctx = ConnectionContext;
        let mut frame = vec![0u8; 20];

        // Should not panic
        protocol.before_send(&mut frame, &mut ctx);
    }
}
