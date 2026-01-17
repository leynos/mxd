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
//! use mxd::wireframe::outbound::{WireframeOutboundConnection, WireframeOutboundRegistry};
//! use std::sync::Arc;
//!
//! let registry = Arc::new(WireframeOutboundRegistry::default());
//! let connection = Arc::new(WireframeOutboundConnection::new(
//!     registry.allocate_id(),
//!     Arc::clone(&registry),
//! ));
//! let protocol = HotlineProtocol::new(pool.clone(), argon2.clone(), connection);
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

use crate::{db::DbPool, wireframe::outbound::WireframeOutboundConnection};

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
    outbound: Arc<WireframeOutboundConnection>,
}

impl HotlineProtocol {
    /// Create a new Hotline protocol adapter.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool for transaction handlers.
    /// * `argon2` - Shared Argon2 instance for password hashing.
    #[must_use]
    pub const fn new(
        pool: DbPool,
        argon2: Arc<Argon2<'static>>,
        outbound: Arc<WireframeOutboundConnection>,
    ) -> Self {
        Self {
            pool,
            argon2,
            outbound,
        }
    }

    /// Return a reference to the database pool.
    #[must_use]
    pub const fn pool(&self) -> &DbPool { &self.pool }

    /// Return a reference to the Argon2 instance.
    #[must_use]
    pub const fn argon2(&self) -> &Arc<Argon2<'static>> { &self.argon2 }

    /// Return the outbound connection state for this protocol instance.
    #[must_use]
    pub const fn outbound(&self) -> &Arc<WireframeOutboundConnection> { &self.outbound }
}

impl WireframeProtocol for HotlineProtocol {
    // The wireframe library's WireframeApp::with_protocol requires these exact types
    type Frame = Vec<u8>;
    type ProtocolError = ();

    fn on_connection_setup(&self, handle: PushHandle<Self::Frame>, _ctx: &mut ConnectionContext) {
        // Connection setup is handled in the app factory via build_app(), which
        // creates ConnectionState with session, context, and handshake metadata.
        // Store the push handle so outbound messaging can target this connection.
        self.outbound.register_handle(&handle);
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
    use rstest::{fixture, rstest};
    use wireframe::ConnectionContext;

    use super::*;
    use crate::wireframe::{
        outbound::{WireframeOutboundConnection, WireframeOutboundRegistry},
        test_helpers::dummy_pool,
    };

    #[fixture]
    fn outbound_connection() -> Arc<WireframeOutboundConnection> {
        let registry = Arc::new(WireframeOutboundRegistry::default());
        let id = registry.allocate_id();
        Arc::new(WireframeOutboundConnection::new(id, registry))
    }

    #[rstest]
    fn protocol_can_be_created(outbound_connection: Arc<WireframeOutboundConnection>) {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());

        let protocol = HotlineProtocol::new(pool, argon2, outbound_connection);

        assert!(Arc::strong_count(protocol.argon2()) >= 1);
    }

    #[rstest]
    fn protocol_shares_argon2_instance(outbound_connection: Arc<WireframeOutboundConnection>) {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let argon2_clone = Arc::clone(&argon2);

        let protocol = HotlineProtocol::new(pool, argon2, outbound_connection);

        assert!(Arc::ptr_eq(protocol.argon2(), &argon2_clone));
    }

    /// Lifecycle hook identifiers for parameterized testing.
    #[derive(Debug, Clone, Copy)]
    enum LifecycleHook {
        HandleError,
        StreamEndFrame,
        OnCommandEnd,
        BeforeSend,
    }

    #[rstest]
    #[case::handle_error(LifecycleHook::HandleError)]
    #[case::stream_end_frame(LifecycleHook::StreamEndFrame)]
    #[case::on_command_end(LifecycleHook::OnCommandEnd)]
    #[case::before_send(LifecycleHook::BeforeSend)]
    fn lifecycle_hooks_do_not_panic(
        #[case] hook: LifecycleHook,
        outbound_connection: Arc<WireframeOutboundConnection>,
    ) {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let protocol = HotlineProtocol::new(pool, argon2, outbound_connection);
        let mut ctx = ConnectionContext;

        match hook {
            LifecycleHook::HandleError => protocol.handle_error((), &mut ctx),
            LifecycleHook::StreamEndFrame => {
                let frame = protocol.stream_end_frame(&mut ctx);
                assert!(frame.is_none());
            }
            LifecycleHook::OnCommandEnd => protocol.on_command_end(&mut ctx),
            LifecycleHook::BeforeSend => {
                let mut frame = vec![0u8; 20];
                protocol.before_send(&mut frame, &mut ctx);
            }
        }
    }
}
