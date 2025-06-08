use std::net::SocketAddr;

use crate::commands::Command;
use crate::db::DbPool;
use crate::transaction::{Transaction, parse_transaction};

/// Per-connection context used by `handle_request`.
pub struct Context {
    pub peer: SocketAddr,
    pub pool: DbPool,
}

/// Session state for a single connection.
pub struct Session {
    pub user_id: Option<i32>,
}

impl Session {
    pub fn new() -> Self {
        Self { user_id: None }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    pub fn new(peer: SocketAddr, pool: DbPool) -> Self {
        Self { peer, pool }
    }
}

/// Parse and handle a single request frame without performing network I/O.
pub async fn handle_request(
    ctx: &Context,
    session: &mut Session,
    frame: &[u8],
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let tx = parse_transaction(frame)?;
    let cmd = Command::from_transaction(tx)?;
    let reply = cmd.process(ctx.peer, ctx.pool.clone(), session).await?;
    Ok(reply)
}
