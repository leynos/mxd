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
//! Transaction processing is integrated through middleware registered on
//! the `WireframeApp`. The middleware intercepts all frames, processes them
//! through the domain command dispatcher, and writes the reply bytes.

use std::{
    convert::Infallible,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use argon2::Argon2;
use async_trait::async_trait;
use tracing::{error, warn};
use wireframe::{
    app::Envelope,
    middleware::{HandlerService, Service, ServiceRequest, ServiceResponse, Transform},
};

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

/// Middleware for processing Hotline transactions.
///
/// This middleware intercepts all incoming frames, processes them through the
/// domain command dispatcher, and writes the reply bytes to the response. It
/// holds the database pool and session state directly rather than using
/// thread-local storage.
///
/// # Wireframe Integration
///
/// Unlike `from_fn` middleware, this struct implements `Transform` with
/// `Output = HandlerService<E>`, making it compatible with `WireframeApp::wrap()`.
/// The wrapped service processes transactions and returns the transformed
/// `HandlerService` expected by the middleware pipeline.
#[derive(Clone)]
pub struct TransactionMiddleware {
    pool: DbPool,
    session: Arc<tokio::sync::Mutex<crate::handler::Session>>,
}

impl TransactionMiddleware {
    /// Create a new transaction middleware with the given pool and session.
    #[must_use]
    pub const fn new(
        pool: DbPool,
        session: Arc<tokio::sync::Mutex<crate::handler::Session>>,
    ) -> Self {
        Self { pool, session }
    }
}

/// Inner service that processes transactions with the pool and session passed in.
struct TransactionService<S> {
    inner: S,
    pool: DbPool,
    session: Arc<tokio::sync::Mutex<crate::handler::Session>>,
}

#[async_trait]
impl<S> Service for TransactionService<S>
where
    S: Service<Error = Infallible> + Send + Sync,
{
    type Error = Infallible;

    async fn call(&self, req: ServiceRequest) -> Result<ServiceResponse, Self::Error> {
        use crate::wireframe::connection::current_peer;

        let peer = current_peer().unwrap_or_else(|| {
            warn!("peer address missing in middleware; using default");
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)
        });

        let frame = req.frame().to_vec();
        let reply_bytes = {
            let mut session_guard = self.session.lock().await;
            process_transaction_bytes(&frame, peer, self.pool.clone(), &mut session_guard).await
        };

        // Call inner service to propagate through the chain, then replace the response frame
        let mut response = self.inner.call(req).await?;
        response.frame_mut().clear();
        response.frame_mut().extend_from_slice(&reply_bytes);
        Ok(response)
    }
}

#[async_trait]
impl Transform<HandlerService<Envelope>> for TransactionMiddleware {
    type Output = HandlerService<Envelope>;

    async fn transform(&self, service: HandlerService<Envelope>) -> Self::Output {
        let id = service.id();
        let wrapped = TransactionService {
            inner: service,
            pool: self.pool.clone(),
            session: Arc::clone(&self.session),
        };
        HandlerService::from_service(id, wrapped)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::{
        transaction::HEADER_LEN,
        wireframe::test_helpers::{dummy_pool, transaction_bytes},
    };

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

    /// Error code indicating a permission failure.
    const ERR_PERMISSION: u32 = 1;

    /// Parameterized test covering error reply scenarios.
    ///
    /// Each case verifies that `error_reply` correctly constructs a reply with:
    /// - `is_reply` set to 1
    /// - The original transaction type preserved
    /// - The original transaction ID preserved
    /// - The specified error code applied
    /// - An empty payload
    #[rstest]
    #[case::creates_valid_transaction(107, 12345, 1)]
    #[case::preserves_transaction_id(200, 99999, ERR_INTERNAL)]
    #[case::invalid_frame_returns_internal_error(0, 0, ERR_INTERNAL)]
    #[case::unknown_type_returns_internal_error(65535, 1, ERR_INTERNAL)]
    #[case::permission_error_preserves_type(200, 2, ERR_PERMISSION)]
    #[case::preserves_id_for_unknown_type(65535, 12345, ERR_INTERNAL)]
    fn error_reply_preserves_header_fields(
        #[case] ty: u16,
        #[case] id: u32,
        #[case] error_code: u32,
    ) {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty,
            id,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let reply =
            error_reply(&header, error_code).expect("error_reply should succeed for valid header");

        assert_eq!(reply.header().is_reply, 1);
        assert_eq!(reply.header().ty, ty);
        assert_eq!(reply.header().id, id);
        assert_eq!(reply.header().error, error_code);
        assert!(reply.payload().is_empty());
    }

    /// Tests that malformed input returns an error with ERR_INTERNAL.
    #[rstest]
    fn handle_parse_error_returns_internal_error() {
        let result = handle_parse_error("simulated parse error");

        // Should produce a valid transaction header + empty payload
        assert!(
            result.len() >= HEADER_LEN,
            "response too short to contain header"
        );

        let reply_header = FrameHeader::from_bytes(
            result[..HEADER_LEN]
                .try_into()
                .expect("header slice should be exact size"),
        );
        assert_eq!(reply_header.is_reply, 1);
        assert_eq!(reply_header.error, ERR_INTERNAL);
        assert_eq!(reply_header.data_size, 0);
    }

    /// Tests that command parse errors preserve the original header fields.
    #[rstest]
    fn handle_command_parse_error_preserves_id() {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 200,
            id: 54321,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let result = handle_command_parse_error("simulated command error", &header);

        let reply_header = FrameHeader::from_bytes(
            result[..HEADER_LEN]
                .try_into()
                .expect("header slice should be exact size"),
        );
        assert_eq!(reply_header.is_reply, 1);
        assert_eq!(reply_header.ty, 200);
        assert_eq!(reply_header.id, 54321);
        assert_eq!(reply_header.error, ERR_INTERNAL);
    }

    /// Tests that transaction_to_bytes correctly serializes a transaction.
    #[rstest]
    fn transaction_to_bytes_roundtrip() {
        let tx = Transaction {
            header: FrameHeader {
                flags: 0,
                is_reply: 1,
                ty: 107,
                id: 999,
                error: 0,
                total_size: 5,
                data_size: 5,
            },
            payload: b"hello".to_vec(),
        };

        let bytes = transaction_to_bytes(&tx);

        assert_eq!(bytes.len(), HEADER_LEN + 5);
        let parsed_header = FrameHeader::from_bytes(
            bytes[..HEADER_LEN]
                .try_into()
                .expect("header slice should be exact size"),
        );
        assert_eq!(parsed_header.id, 999);
        assert_eq!(parsed_header.ty, 107);
        assert_eq!(&bytes[HEADER_LEN..], b"hello");
    }

    /// Tests that error_transaction produces correct header values.
    #[rstest]
    fn error_transaction_sets_reply_flag() {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 370,
            id: 42,
            error: 0,
            total_size: 100,
            data_size: 100,
        };

        let err_tx = error_transaction(&header, ERR_INTERNAL);

        assert_eq!(err_tx.header.is_reply, 1);
        assert_eq!(err_tx.header.ty, 370);
        assert_eq!(err_tx.header.id, 42);
        assert_eq!(err_tx.header.error, ERR_INTERNAL);
        assert!(err_tx.payload.is_empty());
    }

    /// Tests that truncated input returns error.
    #[rstest]
    #[tokio::test]
    async fn process_transaction_bytes_truncated_input() {
        let pool = dummy_pool();
        let mut session = Session::default();
        let peer = "127.0.0.1:12345".parse().expect("valid address");

        // Send only 10 bytes (less than HEADER_LEN = 20)
        let truncated = vec![0u8; 10];
        let result = process_transaction_bytes(&truncated, peer, pool, &mut session).await;

        // Should return an error transaction
        assert!(result.len() >= HEADER_LEN);
        let reply_header = FrameHeader::from_bytes(
            result[..HEADER_LEN]
                .try_into()
                .expect("header slice should be exact size"),
        );
        assert_eq!(reply_header.error, ERR_INTERNAL);
    }

    /// Error code returned for unknown transaction types.
    ///
    /// This matches the behavior in `commands.rs::handle_unknown`.
    const ERR_UNKNOWN_TYPE: u32 = 1;

    /// Tests that unknown transaction type returns error code 1.
    #[rstest]
    #[tokio::test]
    async fn process_transaction_bytes_unknown_type() {
        let pool = dummy_pool();
        let mut session = Session::default();
        let peer = "127.0.0.1:12345".parse().expect("valid address");

        // Create a transaction with unknown type (65535)
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 65535, // Unknown type
            id: 123,
            error: 0,
            total_size: 0,
            data_size: 0,
        };
        let frame = transaction_bytes(&header, &[]);

        let result = process_transaction_bytes(&frame, peer, pool, &mut session).await;

        let reply_header = FrameHeader::from_bytes(
            result[..HEADER_LEN]
                .try_into()
                .expect("header slice should be exact size"),
        );
        assert_eq!(reply_header.is_reply, 1);
        assert_eq!(reply_header.id, 123, "transaction ID should be preserved");
        assert_eq!(reply_header.error, ERR_UNKNOWN_TYPE);
    }

    /// Tests that middleware can be created with pool and session.
    #[rstest]
    fn transaction_middleware_can_be_created() {
        let pool = dummy_pool();
        let session = Arc::new(tokio::sync::Mutex::new(Session::default()));

        let middleware = TransactionMiddleware::new(pool, session);

        // Just verify it can be cloned (required for middleware usage)
        let _cloned = middleware.clone();
    }
}

/// BDD tests for wireframe routing behavior.
///
/// These tests verify that the routing middleware correctly dispatches
/// transactions to handlers and constructs appropriate replies.
#[cfg(test)]
#[expect(
    clippy::allow_attributes,
    reason = "rustc compiler does not emit expected lint"
)]
#[allow(unused_braces, reason = "rstest-bdd macro expansion produces braces")]
mod bdd {
    use std::cell::RefCell;

    use rstest::fixture;
    use rstest_bdd_macros::{given, scenario, then, when};
    use tokio::runtime::Runtime;

    use super::*;
    use crate::{
        transaction::HEADER_LEN,
        wireframe::test_helpers::{dummy_pool, transaction_bytes},
    };

    /// World for routing BDD tests.
    struct RoutingWorld {
        rt: Runtime,
        pool: DbPool,
        reply: RefCell<Option<Vec<u8>>>,
    }

    impl RoutingWorld {
        fn new() -> Self {
            Self {
                rt: Runtime::new().expect("runtime"),
                pool: dummy_pool(),
                reply: RefCell::new(None),
            }
        }

        fn send_transaction(&self, frame: Vec<u8>) {
            let peer = "127.0.0.1:12345".parse().expect("valid address");
            let pool = self.pool.clone();
            let reply = self.rt.block_on(async {
                let mut session = Session::default();
                process_transaction_bytes(&frame, peer, pool, &mut session).await
            });
            self.reply.borrow_mut().replace(reply);
        }

        fn reply_header(&self) -> FrameHeader {
            let reply = self.reply.borrow();
            let bytes = reply.as_ref().expect("no reply received");
            FrameHeader::from_bytes(
                bytes[..HEADER_LEN]
                    .try_into()
                    .expect("reply too short for header"),
            )
        }
    }

    #[fixture]
    fn world() -> RoutingWorld { RoutingWorld::new() }

    #[given("a wireframe server handling transactions")]
    fn given_server(world: &RoutingWorld) {
        // World is already set up with a dummy pool
        let _ = world;
    }

    #[when("I send a transaction with unknown type 65535")]
    fn when_unknown_type(world: &RoutingWorld) {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 65535,
            id: 1,
            error: 0,
            total_size: 0,
            data_size: 0,
        };
        world.send_transaction(transaction_bytes(&header, &[]));
    }

    #[when("I send a truncated frame of 10 bytes")]
    fn when_truncated(world: &RoutingWorld) { world.send_transaction(vec![0u8; 10]); }

    #[when("I send a transaction with unknown type 65535 and ID {id}")]
    fn when_unknown_with_id(world: &RoutingWorld, id: u32) {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 65535,
            id,
            error: 0,
            total_size: 0,
            data_size: 0,
        };
        world.send_transaction(transaction_bytes(&header, &[]));
    }

    #[then("the reply has error code {code}")]
    fn then_error_code(world: &RoutingWorld, code: u32) {
        let header = world.reply_header();
        assert_eq!(header.error, code);
    }

    #[then("the reply has transaction ID {id}")]
    fn then_transaction_id(world: &RoutingWorld, id: u32) {
        let header = world.reply_header();
        assert_eq!(header.id, id);
    }

    #[then("the reply has transaction type {ty}")]
    fn then_transaction_type(world: &RoutingWorld, ty: u16) {
        let header = world.reply_header();
        assert_eq!(header.ty, ty);
    }

    #[scenario(path = "tests/features/wireframe_routing.feature", index = 0)]
    fn routes_unknown_type(world: RoutingWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_routing.feature", index = 1)]
    fn routes_truncated_frame(world: RoutingWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_routing.feature", index = 2)]
    fn preserves_transaction_id(world: RoutingWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_routing.feature", index = 3)]
    fn preserves_transaction_type(world: RoutingWorld) { let _ = world; }
}
