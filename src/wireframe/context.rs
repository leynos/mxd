//! Per-connection state for Wireframe protocol handling.
//!
//! This module defines the [`ConnectionState`] struct that holds session,
//! context, and handshake metadata for each connected client. The state is
//! created during connection setup and passed to route handlers.

use crate::{
    handler::{Context, Session},
    wireframe::connection::HandshakeMetadata,
};

/// Per-connection state managed by the protocol adapter.
///
/// This struct bundles all the state required to process transactions for a
/// single client connection, including authentication state, database access,
/// and protocol-negotiated metadata.
///
/// Fields are public to allow direct access and avoid trivial accessor
/// indirection. Use `context.peer` and `context.pool.clone()` rather than
/// separate delegation methods.
#[derive(Clone)]
pub struct ConnectionState {
    /// Domain session tracking authentication state.
    pub session: Session,
    /// Connection context with database pool and crypto.
    pub context: Context,
    /// Handshake metadata for compatibility decisions.
    pub handshake: HandshakeMetadata,
}

impl ConnectionState {
    /// Create a new connection state for an incoming connection.
    ///
    /// The session starts unauthenticated; login handlers update the session
    /// state upon successful authentication.
    #[must_use]
    pub fn new(context: Context, handshake: HandshakeMetadata) -> Self {
        Self {
            session: Session::default(),
            context,
            handshake,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use argon2::Argon2;
    use rstest::rstest;

    use super::*;
    use crate::wireframe::test_helpers::dummy_pool;

    #[rstest]
    fn connection_state_starts_unauthenticated() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "127.0.0.1:9000".parse().expect("valid addr");
        let context = Context::new(peer, pool, argon2);
        let handshake = HandshakeMetadata::default();

        let state = ConnectionState::new(context, handshake);

        assert!(state.session.user_id.is_none());
    }

    #[rstest]
    fn connection_state_carries_peer_address() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "192.168.1.1:5500".parse().expect("valid addr");
        let context = Context::new(peer, pool, argon2);
        let handshake = HandshakeMetadata::default();

        let state = ConnectionState::new(context, handshake);

        assert_eq!(state.context.peer, peer);
    }

    #[rstest]
    fn connection_state_session_can_be_mutated() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "127.0.0.1:9001".parse().expect("valid addr");
        let context = Context::new(peer, pool, argon2);
        let handshake = HandshakeMetadata::default();

        let mut state = ConnectionState::new(context, handshake);
        state.session.user_id = Some(42);

        assert_eq!(state.session.user_id, Some(42));
    }

    #[rstest]
    fn connection_state_preserves_handshake_metadata() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "127.0.0.1:9002".parse().expect("valid addr");
        let context = Context::new(peer, pool, argon2);
        let handshake = HandshakeMetadata {
            sub_protocol: u32::from_be_bytes(*b"CHAT"),
            version: 1,
            sub_version: 7,
        };

        let state = ConnectionState::new(context, handshake.clone());

        assert_eq!(&state.handshake, &handshake);
    }
}
