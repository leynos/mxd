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
//! 4. Execute via `Command::process_with_outbound()` using routing context
//! 5. Convert reply `Transaction` to bytes
//!
//! # Registration
//!
//! Transaction processing is integrated through middleware registered on
//! the `WireframeApp`. The middleware intercepts all frames, processes them
//! through the domain command dispatcher, and writes the reply bytes.

use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use wireframe::{
    app::Envelope,
    middleware::{HandlerService, Service, ServiceRequest, ServiceResponse, Transform},
};

#[cfg(test)]
use crate::wireframe::codec::HotlineTransaction;
use crate::{
    commands::{Command, CommandContext},
    db::DbPool,
    handler::Session,
    server::outbound::{OutboundMessaging, ReplyBuffer},
    transaction::{FrameHeader, Transaction, parse_transaction},
    wireframe::compat::XorCompatibility,
};
#[cfg(test)]
use crate::{header_util::reply_header, transaction::Transaction};

mod reply_builder;

use reply_builder::ReplyBuilder;

/// Error code for internal server failures.
const ERR_INTERNAL: u32 = 3;

/// Routing context used when dispatching a frame.
pub struct RouteContext<'a> {
    /// Remote peer socket address.
    pub peer: SocketAddr,
    /// Database connection pool.
    pub pool: DbPool,
    /// Mutable session state for the connection.
    pub session: &'a mut Session,
    /// Outbound messaging adapter for push notifications.
    pub messaging: &'a dyn OutboundMessaging,
    /// XOR compatibility state for this connection.
    pub compat: &'a XorCompatibility,
}

#[cfg(test)]
mod dispatch_spy {
    //! Captures dispatch details for transaction routing tests.

    use std::{cell::RefCell, net::SocketAddr};

    use crate::transaction::FrameHeader;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct DispatchRecord {
        pub(super) peer: SocketAddr,
        pub(super) ty: u16,
        pub(super) id: u32,
    }

    thread_local! {
        static RECORDS: RefCell<Vec<DispatchRecord>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn record(peer: SocketAddr, header: &FrameHeader) {
        RECORDS.with(|records| {
            records.borrow_mut().push(DispatchRecord {
                peer,
                ty: header.ty,
                id: header.id,
            });
        });
    }

    pub(super) fn take() -> Vec<DispatchRecord> {
        RECORDS.with(|records| std::mem::take(&mut *records.borrow_mut()))
    }

    pub(super) fn clear() { RECORDS.with(|records| records.borrow_mut().clear()); }
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
/// * `context` - Routing context containing peer, database, session, and outbound messaging
///   adapters.
///
/// # Returns
///
/// Raw bytes containing the reply transaction, or an error transaction if
/// processing fails.
pub async fn process_transaction_bytes(frame: &[u8], context: RouteContext<'_>) -> Vec<u8> {
    let RouteContext {
        peer,
        pool,
        session,
        messaging,
        compat,
    } = context;
    // Parse the frame as a domain Transaction
    let tx = match parse_transaction(frame) {
        Ok(tx) => tx,
        Err(e) => return handle_parse_error(peer, frame, e),
    };

    let header = tx.header.clone();
    let tx = match compat.decode_payload(&tx.payload) {
        Ok(payload) => Transaction { payload, ..tx },
        Err(e) => return handle_command_parse_error(peer, &header, e),
    };

    // Parse into Command and process
    let cmd = match Command::from_transaction(tx) {
        Ok(cmd) => cmd,
        Err(e) => return handle_command_parse_error(peer, &header, e),
    };

    #[cfg(test)]
    dispatch_spy::record(peer, &header);

    let mut transport = ReplyBuffer::new();
    let command_context = CommandContext {
        peer,
        pool,
        session,
        transport: &mut transport,
        messaging,
    };
    match cmd.process_with_outbound(command_context).await {
        Ok(()) => transport.take_reply().map_or_else(
            || handle_missing_reply(peer, &header),
            |reply| reply.to_bytes(),
        ),
        Err(e) => handle_process_error(peer, &header, e),
    }
}

/// Convert a transaction to raw bytes.
#[cfg(test)]
fn transaction_to_bytes(tx: &Transaction) -> Vec<u8> { tx.to_bytes() }

/// Build an error reply transaction from a header and error code.
#[cfg(test)]
fn error_transaction(header: &FrameHeader, error_code: u32) -> Transaction {
    Transaction {
        header: reply_header(header, error_code, 0),
        payload: Vec::new(),
    }
}

/// Handle transaction parse errors by returning an error reply.
fn handle_parse_error(peer: SocketAddr, frame: &[u8], e: impl std::fmt::Display) -> Vec<u8> {
    ReplyBuilder::from_frame(peer, frame).parse_error(e, ERR_INTERNAL)
}

/// Handle command parsing errors by returning an error reply.
fn handle_command_parse_error(
    peer: SocketAddr,
    header: &FrameHeader,
    e: impl std::fmt::Display,
) -> Vec<u8> {
    ReplyBuilder::from_header(peer, header).command_parse_error(e, ERR_INTERNAL)
}

/// Handle command processing errors by returning an error reply.
fn handle_process_error(
    peer: SocketAddr,
    header: &FrameHeader,
    e: impl std::fmt::Display,
) -> Vec<u8> {
    ReplyBuilder::from_header(peer, header).process_error(e, ERR_INTERNAL)
}

/// Handle missing replies by returning an error reply.
fn handle_missing_reply(peer: SocketAddr, header: &FrameHeader) -> Vec<u8> {
    ReplyBuilder::from_header(peer, header).missing_reply(ERR_INTERNAL)
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
#[cfg(test)]
fn error_reply(
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
/// holds the database pool, peer address, and session state directly rather
/// than using thread-local storage.
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
    peer: SocketAddr,
    messaging: Arc<dyn OutboundMessaging>,
    compat: Arc<XorCompatibility>,
}

impl TransactionMiddleware {
    /// Create a new transaction middleware with the given pool and session.
    #[must_use]
    pub const fn new(
        pool: DbPool,
        session: Arc<tokio::sync::Mutex<crate::handler::Session>>,
        peer: SocketAddr,
        messaging: Arc<dyn OutboundMessaging>,
        compat: Arc<XorCompatibility>,
    ) -> Self {
        Self {
            pool,
            session,
            peer,
            messaging,
            compat,
        }
    }
}

/// Inner service that processes transactions with the pool and session passed in.
struct TransactionHandler {
    inner: HandlerService<Envelope>,
    pool: DbPool,
    session: Arc<tokio::sync::Mutex<crate::handler::Session>>,
    peer: SocketAddr,
    messaging: Arc<dyn OutboundMessaging>,
    compat: Arc<XorCompatibility>,
}

#[async_trait]
impl Service for TransactionHandler {
    type Error = Infallible;

    async fn call(&self, req: ServiceRequest) -> Result<ServiceResponse, Self::Error> {
        let reply_bytes = {
            let mut session_guard = self.session.lock().await;
            process_transaction_bytes(
                req.frame(),
                RouteContext {
                    peer: self.peer,
                    pool: self.pool.clone(),
                    session: &mut session_guard,
                    messaging: self.messaging.as_ref(),
                    compat: &self.compat,
                },
            )
            .await
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
        let wrapped = TransactionHandler {
            inner: service,
            pool: self.pool.clone(),
            session: Arc::clone(&self.session),
            peer: self.peer,
            messaging: Arc::clone(&self.messaging),
            compat: Arc::clone(&self.compat),
        };
        HandlerService::from_service(id, wrapped)
    }
}

#[cfg(test)]
mod tests;
