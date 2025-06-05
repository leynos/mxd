use std::net::SocketAddr;

use crate::commands::Command;
use crate::db::DbPool;
use crate::transaction::{Transaction, parse_transaction};

/// Per-connection context used by `handle_request`.
pub struct Context {
    pub peer: SocketAddr,
    pub pool: DbPool,
}

impl Context {
    pub fn new(peer: SocketAddr, pool: DbPool) -> Self {
        Self { peer, pool }
    }
}

/// Parse and handle a single request frame without performing network I/O.
pub async fn handle_request(
    ctx: &Context,
    frame: &[u8],
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let tx = parse_transaction(frame)?;
    let cmd = Command::from_transaction(tx)?;
    let reply = cmd.process(ctx.peer, ctx.pool.clone()).await?;
    Ok(reply)
}
