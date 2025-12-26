//! Route handlers for Hotline transaction routing.
//!
//! This module provides route handler functions that bridge incoming
//! [`HotlineTransaction`] frames to the domain [`Command`] dispatcher.
//! Each handler extracts the transaction from raw bytes, converts it to
//! a domain command, processes it, and returns the reply.
//!
//! # Architecture
//!
//! Route handlers follow a consistent pattern:
//!
//! 1. Parse raw bytes into `HotlineTransaction`
//! 2. Convert to domain `Transaction` via `From` impl
//! 3. Parse into `Command` via `Command::from_transaction()`
//! 4. Execute via `Command::process()` with context and session
//! 5. Convert reply `Transaction` to bytes
//!
//! # Registration
//!
//! Transaction processing is integrated through the protocol adapter's
//! lifecycle hooks and the server's frame handling loop.

use std::sync::Arc;

use argon2::Argon2;
use tracing::{error, warn};

use crate::{
    commands::Command,
    db::DbPool,
    handler::Session,
    header_util::reply_header,
    transaction::{FrameHeader, Transaction, parse_transaction},
    wireframe::{codec::HotlineTransaction, connection::HandshakeMetadata},
};

/// Error code for internal server failures.
const ERR_INTERNAL: u32 = 3;

/// Shared state passed to route handlers via app data.
///
/// This struct aggregates the shared resources needed by transaction handlers,
/// extracted from `WireframeApp` data during request processing.
#[derive(Clone)]
pub struct RouteState {
    /// Database connection pool.
    pub pool: DbPool,
    /// Shared Argon2 instance for password hashing.
    pub argon2: Arc<Argon2<'static>>,
    /// Handshake metadata for the connection.
    pub handshake: HandshakeMetadata,
}

impl RouteState {
    /// Create a new route state from app data components.
    #[must_use]
    pub const fn new(
        pool: DbPool,
        argon2: Arc<Argon2<'static>>,
        handshake: HandshakeMetadata,
    ) -> Self {
        Self {
            pool,
            argon2,
            handshake,
        }
    }
}

/// Per-connection mutable state for session tracking.
///
/// This wrapper holds the session state that is mutated across transactions
/// within a single connection. It is stored in app data and passed to handlers.
#[derive(Clone, Default)]
pub struct SessionState {
    /// Domain session tracking authentication state.
    session: Session,
}

impl SessionState {
    /// Create a new session state with default (unauthenticated) session.
    #[must_use]
    pub fn new() -> Self { Self::default() }

    /// Return a reference to the session.
    #[must_use]
    pub const fn session(&self) -> &Session { &self.session }

    /// Return a mutable reference to the session.
    pub const fn session_mut(&mut self) -> &mut Session { &mut self.session }
}

/// Process a Hotline transaction from raw bytes and return the reply bytes.
///
/// This function implements the core routing logic. It parses the raw bytes
/// into a domain transaction, dispatches to the appropriate command handler,
/// and returns the reply as raw bytes.
///
/// # Arguments
///
/// * `frame` - Raw transaction bytes (header + payload).
/// * `peer` - The remote peer socket address.
/// * `pool` - Database connection pool.
/// * `session` - Mutable session state for the connection.
///
/// # Returns
///
/// Raw bytes containing the reply transaction, or an error transaction if
/// processing fails.
pub async fn process_transaction_bytes(
    frame: &[u8],
    peer: std::net::SocketAddr,
    pool: DbPool,
    session: &mut Session,
) -> Vec<u8> {
    // Parse the frame as a domain Transaction
    let tx = match parse_transaction(frame) {
        Ok(tx) => tx,
        Err(e) => return handle_parse_error(e),
    };

    let header = tx.header.clone();

    // Parse into Command and process
    let cmd = match Command::from_transaction(tx) {
        Ok(cmd) => cmd,
        Err(e) => return handle_command_parse_error(e, &header),
    };

    match cmd.process(peer, pool, session).await {
        Ok(reply) => transaction_to_bytes(&reply),
        Err(e) => handle_process_error(e, &header),
    }
}

/// Convert a transaction to raw bytes.
fn transaction_to_bytes(tx: &Transaction) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(crate::transaction::HEADER_LEN + tx.payload.len());
    let mut header_buf = [0u8; crate::transaction::HEADER_LEN];
    tx.header.write_bytes(&mut header_buf);
    bytes.extend_from_slice(&header_buf);
    bytes.extend_from_slice(&tx.payload);
    bytes
}

/// Build an error reply transaction from a header and error code.
fn error_transaction(header: &FrameHeader, error_code: u32) -> Transaction {
    Transaction {
        header: reply_header(header, error_code, 0),
        payload: Vec::new(),
    }
}

/// Handle transaction parse errors by returning an error reply.
fn handle_parse_error(e: impl std::fmt::Display) -> Vec<u8> {
    warn!(error = %e, "failed to parse transaction from bytes");
    let header = FrameHeader {
        flags: 0,
        is_reply: 1,
        ty: 0,
        id: 0,
        error: ERR_INTERNAL,
        total_size: 0,
        data_size: 0,
    };
    transaction_to_bytes(&error_transaction(&header, ERR_INTERNAL))
}

/// Handle command parsing errors by returning an error reply.
fn handle_command_parse_error(e: impl std::fmt::Display, header: &FrameHeader) -> Vec<u8> {
    warn!(error = %e, "failed to parse command from transaction");
    transaction_to_bytes(&error_transaction(header, ERR_INTERNAL))
}

/// Handle command processing errors by returning an error reply.
fn handle_process_error(e: impl std::fmt::Display, header: &FrameHeader) -> Vec<u8> {
    error!(error = %e, "command processing failed");
    transaction_to_bytes(&error_transaction(header, ERR_INTERNAL))
}

/// Build an error reply as a `HotlineTransaction`.
///
/// # Errors
///
/// Returns a [`TransactionError`] if the codec fails to create the transaction.
/// This is unexpected for empty payloads and would indicate a bug in the codec
/// implementation.
///
/// [`TransactionError`]: crate::transaction::TransactionError
pub fn error_reply(
    header: &FrameHeader,
    error_code: u32,
) -> Result<HotlineTransaction, crate::transaction::TransactionError> {
    let tx = error_transaction(header, error_code);
    HotlineTransaction::try_from(tx)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::wireframe::test_helpers::dummy_pool;

    #[rstest]
    fn route_state_can_be_created() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let handshake = HandshakeMetadata::default();

        let state = RouteState::new(pool, argon2, handshake);

        assert!(Arc::strong_count(&state.argon2) >= 1);
    }

    #[rstest]
    fn session_state_starts_unauthenticated() {
        let state = SessionState::new();

        assert!(state.session().user_id.is_none());
    }

    #[rstest]
    fn session_state_can_be_mutated() {
        let mut state = SessionState::new();
        state.session_mut().user_id = Some(42);

        assert_eq!(state.session().user_id, Some(42));
    }

    #[rstest]
    fn error_reply_creates_valid_transaction() {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 12345,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let reply = error_reply(&header, 1).expect("error_reply should succeed for valid header");

        assert_eq!(reply.header().is_reply, 1);
        assert_eq!(reply.header().ty, 107);
        assert_eq!(reply.header().id, 12345);
        assert_eq!(reply.header().error, 1);
        assert!(reply.payload().is_empty());
    }

    #[rstest]
    fn error_reply_preserves_transaction_id() {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 200,
            id: 99999,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let reply = error_reply(&header, ERR_INTERNAL)
            .expect("error_reply should succeed for valid header");

        assert_eq!(reply.header().id, 99999);
        assert_eq!(reply.header().error, ERR_INTERNAL);
    }
}

/// Additional unit tests covering error reply scenarios.
///
/// These tests verify the behaviour of `error_reply` across various routing
/// scenarios without the overhead of BDD scaffolding. The scenarios correspond
/// to those previously defined in `tests/features/wireframe_protocol_routing.feature`.
#[cfg(test)]
mod error_reply_scenarios {
    use rstest::rstest;

    use super::*;

    /// Error code indicating a permission failure.
    const ERR_PERMISSION: u32 = 1;

    #[rstest]
    fn invalid_frame_returns_internal_error() {
        // Simulates routing an unparseable frame: the router creates an error
        // reply with the internal error code.
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 0,
            id: 0,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let reply = error_reply(&header, ERR_INTERNAL)
            .expect("error_reply should succeed for valid header");

        assert_eq!(reply.header().error, ERR_INTERNAL);
        assert_eq!(reply.header().is_reply, 1);
    }

    #[rstest]
    fn unknown_type_returns_internal_error() {
        // Simulates routing a transaction with an unrecognised command type.
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 65535, // Unknown type
            id: 1,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let reply = error_reply(&header, ERR_INTERNAL)
            .expect("error_reply should succeed for valid header");

        assert_eq!(reply.header().error, ERR_INTERNAL);
        assert_eq!(reply.header().is_reply, 1);
        assert_eq!(reply.header().ty, 65535);
    }

    #[rstest]
    fn permission_error_preserves_type() {
        // Simulates an unauthenticated client sending a protected command.
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 200, // GetFileNameList
            id: 2,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let reply = error_reply(&header, ERR_PERMISSION)
            .expect("error_reply should succeed for valid header");

        assert_eq!(reply.header().error, ERR_PERMISSION);
        assert_eq!(reply.header().is_reply, 1);
        assert_eq!(reply.header().ty, 200);
    }

    #[rstest]
    fn error_reply_preserves_id_for_unknown_type() {
        // Verifies transaction ID preservation for error replies.
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 65535,
            id: 12345,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let reply = error_reply(&header, ERR_INTERNAL)
            .expect("error_reply should succeed for valid header");

        assert_eq!(reply.header().id, 12345);
        assert_eq!(reply.header().error, ERR_INTERNAL);
    }
}
