//! Connection-level request processing.
//!
//! The handler owns per-client [`Session`] state and dispatches incoming
//! transactions to [`Command`] processors. Each connection runs in its own
//! asynchronous task.
use std::net::SocketAddr;

use crate::{
    commands::Command,
    db::DbPool,
    transaction::{Transaction, parse_transaction},
};

/// Per-connection context used by `handle_request`.
pub struct Context {
    pub peer: SocketAddr,
    pub pool: DbPool,
}

/// Session state for a single connection.
#[derive(Default)]
pub struct Session {
    pub user_id: Option<i32>,
}

impl Context {
    #[must_use]
    pub fn new(peer: SocketAddr, pool: DbPool) -> Self { Self { peer, pool } }
}

/// Parse and handle a single request frame without performing network I/O.
///
/// # Errors
/// Returns an error if the transaction cannot be parsed or processed.
#[must_use = "handle the result"]
pub async fn handle_request(
    ctx: &Context,
    session: &mut Session,
    frame: &[u8],
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let tx = parse_transaction(frame)?;
    let cmd = Command::from_transaction(tx)?;
    let reply = cmd.process(ctx.peer, ctx.pool.clone(), session).await?;
    Ok(reply)
}
