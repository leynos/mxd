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
//! use mxd::wireframe::{compat::XorCompatibility, protocol::HotlineProtocol};
//! use mxd::wireframe::outbound::{WireframeOutboundConnection, WireframeOutboundRegistry};
//! use std::sync::Arc;
//!
//! let registry = Arc::new(WireframeOutboundRegistry::default());
//! let connection = Arc::new(WireframeOutboundConnection::new(
//!     registry.allocate_id(),
//!     Arc::clone(&registry),
//! ));
//! let compat = Arc::new(XorCompatibility::disabled());
//! let protocol = HotlineProtocol::new(pool.clone(), argon2.clone(), connection, compat);
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

use crate::{
    db::DbPool,
    transaction::{HEADER_LEN, parse_transaction},
    wireframe::{compat::XorCompatibility, outbound::WireframeOutboundConnection},
};

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
    compat: Arc<XorCompatibility>,
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
        compat: Arc<XorCompatibility>,
    ) -> Self {
        Self {
            pool,
            argon2,
            outbound,
            compat,
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

    /// Return the XOR compatibility state for this connection.
    #[must_use]
    pub const fn compat(&self) -> &Arc<XorCompatibility> { &self.compat }
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

    fn before_send(&self, frame: &mut Self::Frame, _ctx: &mut ConnectionContext) {
        if !self.compat.is_enabled() {
            return;
        }
        let Ok(tx) = parse_transaction(frame) else {
            return;
        };
        let Ok(encoded) = self.compat.encode_payload(&tx.payload) else {
            return;
        };
        if encoded == tx.payload {
            return;
        }
        let mut header_buf = [0u8; HEADER_LEN];
        tx.header.write_bytes(&mut header_buf);
        frame.clear();
        frame.extend_from_slice(&header_buf);
        frame.extend_from_slice(&encoded);
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
        compat::XorCompatibility,
        outbound::{WireframeOutboundConnection, WireframeOutboundRegistry},
        test_helpers::dummy_pool,
    };

    fn xor_bytes(data: &[u8]) -> Vec<u8> { data.iter().map(|byte| byte ^ 0xff).collect() }

    #[fixture]
    fn outbound_connection() -> Arc<WireframeOutboundConnection> {
        let registry = Arc::new(WireframeOutboundRegistry::default());
        let id = registry.allocate_id();
        Arc::new(WireframeOutboundConnection::new(id, registry))
    }

    #[fixture]
    fn compat() -> Arc<XorCompatibility> {
        #[expect(
            clippy::allow_attributes,
            reason = "cannot use expect due to macro interaction"
        )]
        #[allow(unused_braces, reason = "rustfmt requires braces")]
        {
            Arc::new(XorCompatibility::disabled())
        }
    }

    #[rstest]
    fn protocol_can_be_created(
        outbound_connection: Arc<WireframeOutboundConnection>,
        compat: Arc<XorCompatibility>,
    ) {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());

        let protocol = HotlineProtocol::new(pool, argon2, outbound_connection, compat);

        assert!(Arc::strong_count(protocol.argon2()) >= 1);
    }

    #[rstest]
    fn protocol_shares_argon2_instance(
        outbound_connection: Arc<WireframeOutboundConnection>,
        compat: Arc<XorCompatibility>,
    ) {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let argon2_clone = Arc::clone(&argon2);

        let protocol = HotlineProtocol::new(pool, argon2, outbound_connection, compat);

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
        compat: Arc<XorCompatibility>,
    ) {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let protocol = HotlineProtocol::new(pool, argon2, outbound_connection, compat);
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

    #[rstest]
    fn before_send_encodes_text_fields_when_enabled(
        outbound_connection: Arc<WireframeOutboundConnection>,
    ) {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let compat = Arc::new(XorCompatibility::enabled());
        let protocol = HotlineProtocol::new(pool, argon2, outbound_connection, compat);
        let mut ctx = ConnectionContext;

        let payload = crate::transaction::encode_params(&[(
            crate::field_id::FieldId::Data,
            b"message".as_ref(),
        )])
        .expect("payload encodes");
        let payload_len = u32::try_from(payload.len()).expect("payload length fits u32");
        let header = crate::transaction::FrameHeader {
            flags: 0,
            is_reply: 1,
            ty: crate::transaction_type::TransactionType::Error.into(),
            id: 9,
            error: 0,
            total_size: payload_len,
            data_size: payload_len,
        };
        let tx = crate::transaction::Transaction { header, payload };
        let mut frame = tx.to_bytes();

        protocol.before_send(&mut frame, &mut ctx);

        let reply = crate::transaction::parse_transaction(&frame).expect("parse reply");
        let params = crate::transaction::decode_params(&reply.payload).expect("decode params");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, crate::field_id::FieldId::Data);
        let decoded = xor_bytes(&params[0].1);
        assert_eq!(decoded, b"message");
    }
}
