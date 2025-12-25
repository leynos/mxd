//! Per-connection state for Wireframe protocol handling.
//!
//! This module defines the [`ConnectionState`] struct that holds session,
//! context, and handshake metadata for each connected client. The state is
//! created during connection setup and passed to route handlers.

use std::{net::SocketAddr, sync::Arc};

use argon2::Argon2;

use crate::{
    db::DbPool,
    handler::{Context, Session},
    wireframe::connection::HandshakeMetadata,
};

/// Per-connection state managed by the protocol adapter.
///
/// This struct bundles all the state required to process transactions for a
/// single client connection, including authentication state, database access,
/// and protocol-negotiated metadata.
#[derive(Clone)]
pub struct ConnectionState {
    /// Domain session tracking authentication state.
    session: Session,
    /// Connection context with database pool and crypto.
    context: Context,
    /// Handshake metadata for compatibility decisions.
    handshake: HandshakeMetadata,
}

impl ConnectionState {
    /// Create a new connection state for an incoming connection.
    ///
    /// The session starts unauthenticated; login handlers update the session
    /// state upon successful authentication.
    #[must_use]
    pub fn new(
        peer: SocketAddr,
        pool: DbPool,
        argon2: Arc<Argon2<'static>>,
        handshake: HandshakeMetadata,
    ) -> Self {
        Self {
            session: Session::default(),
            context: Context::new(peer, pool, argon2),
            handshake,
        }
    }

    /// Return a reference to the session state.
    #[must_use]
    pub const fn session(&self) -> &Session { &self.session }

    /// Return a mutable reference to the session state.
    pub const fn session_mut(&mut self) -> &mut Session { &mut self.session }

    /// Return a reference to the connection context.
    #[must_use]
    pub const fn context(&self) -> &Context { &self.context }

    /// Return a reference to the handshake metadata.
    #[must_use]
    pub const fn handshake(&self) -> &HandshakeMetadata { &self.handshake }

    /// Return the peer socket address.
    #[must_use]
    pub const fn peer(&self) -> SocketAddr { self.context.peer }

    /// Return a clone of the database pool.
    #[must_use]
    pub fn pool(&self) -> DbPool { self.context.pool.clone() }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use diesel_async::pooled_connection::{AsyncDieselConnectionManager, bb8::Pool};
    use rstest::rstest;

    use super::*;
    use crate::db::DbConnection;

    fn dummy_pool() -> DbPool {
        let manager = AsyncDieselConnectionManager::<DbConnection>::new(
            "postgres://example.invalid/mxd-test",
        );
        Pool::builder()
            .max_size(1)
            .min_idle(Some(0))
            .idle_timeout(None::<Duration>)
            .max_lifetime(None::<Duration>)
            .test_on_check_out(false)
            .build_unchecked(manager)
    }

    #[rstest]
    fn connection_state_starts_unauthenticated() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "127.0.0.1:9000".parse().expect("valid addr");
        let handshake = HandshakeMetadata::default();

        let state = ConnectionState::new(peer, pool, argon2, handshake);

        assert!(state.session().user_id.is_none());
    }

    #[rstest]
    fn connection_state_carries_peer_address() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "192.168.1.1:5500".parse().expect("valid addr");
        let handshake = HandshakeMetadata::default();

        let state = ConnectionState::new(peer, pool, argon2, handshake);

        assert_eq!(state.peer(), peer);
    }

    #[rstest]
    fn connection_state_session_can_be_mutated() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "127.0.0.1:9001".parse().expect("valid addr");
        let handshake = HandshakeMetadata::default();

        let mut state = ConnectionState::new(peer, pool, argon2, handshake);
        state.session_mut().user_id = Some(42);

        assert_eq!(state.session().user_id, Some(42));
    }

    #[rstest]
    fn connection_state_preserves_handshake_metadata() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "127.0.0.1:9002".parse().expect("valid addr");
        let handshake = HandshakeMetadata {
            sub_protocol: u32::from_be_bytes(*b"CHAT"),
            version: 1,
            sub_version: 7,
        };

        let state = ConnectionState::new(peer, pool, argon2, handshake.clone());

        assert_eq!(state.handshake(), &handshake);
    }
}
