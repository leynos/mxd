//! Connection-level request processing.
//!
//! The handler owns per-client [`Session`] state and dispatches incoming
//! transactions to [`Command`] processors. Each connection runs in its own
//! asynchronous task.
use std::{net::SocketAddr, sync::Arc};

use argon2::Argon2;

use crate::{
    commands::Command,
    db::DbPool,
    transaction::{Transaction, parse_transaction},
};

/// Per-connection context used by `handle_request`.
pub struct Context {
    pub peer: SocketAddr,
    pub pool: DbPool,
    pub argon2: Arc<Argon2<'static>>,
}

/// Session state for a single connection.
#[derive(Default)]
pub struct Session {
    pub user_id: Option<i32>,
}

impl Context {
    #[must_use]
    pub fn new(peer: SocketAddr, pool: DbPool, argon2: Arc<Argon2<'static>>) -> Self {
        Self { peer, pool, argon2 }
    }
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use diesel_async::pooled_connection::{AsyncDieselConnectionManager, bb8::Pool};

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

    #[tokio::test]
    async fn context_carries_shared_argon2_reference() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let peer: SocketAddr = "127.0.0.1:9001".parse().expect("loopback address");

        let ctx = Context::new(peer, pool, Arc::clone(&argon2));

        assert!(Arc::ptr_eq(&ctx.argon2, &argon2));
        assert_eq!(Arc::strong_count(&argon2), 2);
        assert_eq!(ctx.peer, peer);
    }

    #[tokio::test]
    async fn multiple_contexts_share_single_argon2_instance() {
        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());

        let ctx_a = Context::new(
            "127.0.0.1:9002".parse().expect("loopback"),
            pool.clone(),
            Arc::clone(&argon2),
        );
        let ctx_b = Context::new(
            "127.0.0.1:9003".parse().expect("loopback"),
            pool,
            Arc::clone(&argon2),
        );

        assert!(Arc::ptr_eq(&ctx_a.argon2, &argon2));
        assert!(Arc::ptr_eq(&ctx_b.argon2, &argon2));
        assert_eq!(Arc::strong_count(&argon2), 3);

        drop(ctx_a);
        assert_eq!(Arc::strong_count(&argon2), 2);
        drop(ctx_b);
        assert_eq!(Arc::strong_count(&argon2), 1);
    }
}
