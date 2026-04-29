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
use crate::header_util::reply_header;
#[cfg(test)]
use crate::transaction::Transaction;
#[cfg(test)]
use crate::wireframe::codec::HotlineTransaction;
use crate::{
    db::DbPool,
    presence::PresenceRegistry,
    server::outbound::{OutboundConnectionId, OutboundMessaging},
    transaction::FrameHeader,
    wireframe::router::{RouteContext as RouterRouteContext, WireframeRouter},
};

pub(crate) mod reply_builder;

use reply_builder::ReplyBuilder;

/// Error code for internal server failures.
const ERR_INTERNAL: u32 = 3;

#[cfg(test)]
pub(crate) mod dispatch_spy {
    //! Captures dispatch details for transaction routing tests.

    use std::{cell::RefCell, net::SocketAddr};

    use crate::transaction::FrameHeader;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct DispatchRecord {
        pub(crate) peer: SocketAddr,
        pub(crate) ty: u16,
        pub(crate) id: u32,
    }

    thread_local! {
        static RECORDS: RefCell<Vec<DispatchRecord>> = const { RefCell::new(Vec::new()) };
    }

    pub(crate) fn record(peer: SocketAddr, header: &FrameHeader) {
        RECORDS.with(|records| {
            records.borrow_mut().push(DispatchRecord {
                peer,
                ty: header.ty,
                id: header.id,
            });
        });
    }

    pub(crate) fn take() -> Vec<DispatchRecord> {
        RECORDS.with(|records| std::mem::take(&mut *records.borrow_mut()))
    }

    pub(crate) fn clear() { RECORDS.with(|records| records.borrow_mut().clear()); }
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
pub(crate) fn handle_parse_error(
    peer: SocketAddr,
    frame: &[u8],
    e: impl std::fmt::Display,
) -> Vec<u8> {
    ReplyBuilder::from_frame(peer, frame).parse_error(e, ERR_INTERNAL)
}

/// Handle command parsing errors by returning an error reply.
pub(crate) fn handle_command_parse_error(
    peer: SocketAddr,
    header: &FrameHeader,
    e: impl std::fmt::Display,
) -> Vec<u8> {
    ReplyBuilder::from_header(peer, header).command_parse_error(e, ERR_INTERNAL)
}

/// Handle command processing errors by returning an error reply.
pub(crate) fn handle_process_error(
    peer: SocketAddr,
    header: &FrameHeader,
    e: impl std::fmt::Display,
) -> Vec<u8> {
    ReplyBuilder::from_header(peer, header).process_error(e, ERR_INTERNAL)
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
/// holds a [`WireframeRouter`] that owns the compatibility state, plus the
/// database pool, peer address, and session state directly rather than using
/// thread-local storage.
///
/// # Wireframe Integration
///
/// Unlike `from_fn` middleware, this struct implements `Transform` with
/// `Output = HandlerService<E>`, making it compatible with `WireframeApp::wrap()`.
/// The wrapped service processes transactions and returns the transformed
/// `HandlerService` expected by the middleware pipeline.
#[derive(Clone)]
pub(crate) struct TransactionMiddleware {
    router: WireframeRouter,
    pool: DbPool,
    session: Arc<tokio::sync::Mutex<crate::handler::Session>>,
    peer: SocketAddr,
    messaging: Arc<dyn OutboundMessaging>,
    presence: Arc<PresenceRegistry>,
    presence_connection_id: OutboundConnectionId,
}

/// Construction parameters for [`TransactionMiddleware`].
pub(crate) struct TransactionMiddlewareConfig {
    /// Wireframe router owning compatibility state.
    pub(crate) router: WireframeRouter,
    /// Database connection pool.
    pub(crate) pool: DbPool,
    /// Session state for the connection.
    pub(crate) session: Arc<tokio::sync::Mutex<crate::handler::Session>>,
    /// Remote peer address.
    pub(crate) peer: SocketAddr,
    /// Outbound messaging adapter for push notifications.
    pub(crate) messaging: Arc<dyn OutboundMessaging>,
    /// Shared online presence registry.
    pub(crate) presence: Arc<PresenceRegistry>,
    /// Adapter-owned connection identifier for presence snapshots.
    pub(crate) presence_connection_id: OutboundConnectionId,
}

impl TransactionMiddleware {
    /// Create a new transaction middleware with the given pool and session.
    #[must_use]
    pub(crate) fn new(config: TransactionMiddlewareConfig) -> Self {
        Self {
            router: config.router,
            pool: config.pool,
            session: config.session,
            peer: config.peer,
            messaging: config.messaging,
            presence: config.presence,
            presence_connection_id: config.presence_connection_id,
        }
    }
}

/// Inner service that processes transactions via [`WireframeRouter`].
struct TransactionHandler {
    inner: HandlerService<Envelope>,
    router: WireframeRouter,
    pool: DbPool,
    session: Arc<tokio::sync::Mutex<crate::handler::Session>>,
    peer: SocketAddr,
    messaging: Arc<dyn OutboundMessaging>,
    presence: Arc<PresenceRegistry>,
    presence_connection_id: OutboundConnectionId,
}

#[async_trait]
impl Service for TransactionHandler {
    type Error = Infallible;

    async fn call(&self, req: ServiceRequest) -> Result<ServiceResponse, Self::Error> {
        let reply_bytes = {
            let mut session_guard = self.session.lock().await;
            self.router
                .route(
                    req.frame(),
                    RouterRouteContext {
                        peer: self.peer,
                        pool: self.pool.clone(),
                        session: &mut session_guard,
                        messaging: self.messaging.as_ref(),
                        presence: self.presence.as_ref(),
                        presence_connection_id: self.presence_connection_id,
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
            router: self.router.clone(),
            pool: self.pool.clone(),
            session: Arc::clone(&self.session),
            peer: self.peer,
            messaging: Arc::clone(&self.messaging),
            presence: Arc::clone(&self.presence),
            presence_connection_id: self.presence_connection_id,
        };
        HandlerService::from_service(id, wrapped)
    }
}

#[cfg(test)]
mod tests;
