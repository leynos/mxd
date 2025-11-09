//! Legacy Tokio-based transport adapter.
//!
//! This module hosts the bespoke networking loop that predates the Wireframe
//! migration. It lives inside the library so any binary — the historical CLI
//! daemon, integration smoke tests, or future adapters — can reuse the same
//! listener and handshake implementation without duplicating protocol glue.

use std::{io, net::SocketAddr};

use anyhow::Result;
use thiserror::Error;
use tokio::{
    io::{self as tokio_io, AsyncReadExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
    time::timeout,
};
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use url::Url;

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use crate::db::audit_postgres_features;
#[cfg(feature = "sqlite")]
use crate::db::audit_sqlite_features;
use crate::{
    db::{DbPool, apply_migrations, establish_pool},
    handler::{Context as HandlerContext, Session, handle_request},
    protocol,
    transaction::{TransactionError, TransactionReader, TransactionWriter},
};

/// Errors that prevent the legacy server from starting.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LegacyConfigError {
    #[error("bind address {0} is invalid")]
    InvalidBind(String),
    #[error("database url cannot be empty")]
    EmptyDatabaseUrl,
}

/// Configuration for the legacy Tokio listener.
#[derive(Debug, Clone)]
pub struct LegacyServerConfig {
    bind: SocketAddr,
    database_url: String,
}

impl LegacyServerConfig {
    /// Create a configuration from validated values.
    ///
    /// # Errors
    /// Returns [`LegacyConfigError::EmptyDatabaseUrl`] when the database string
    /// is blank or whitespace.
    pub fn new(
        bind: SocketAddr,
        database_url: impl Into<String>,
    ) -> Result<Self, LegacyConfigError> {
        let database_url = database_url.into();
        if database_url.trim().is_empty() {
            return Err(LegacyConfigError::EmptyDatabaseUrl);
        }
        Ok(Self { bind, database_url })
    }

    /// Parse the bind address from a user-supplied string.
    ///
    /// # Errors
    /// Returns [`LegacyConfigError::InvalidBind`] on parse failures or
    /// [`LegacyConfigError::EmptyDatabaseUrl`] for blank database URLs.
    pub fn from_raw(bind: &str, database_url: &str) -> Result<Self, LegacyConfigError> {
        let addr = bind
            .parse()
            .map_err(|_| LegacyConfigError::InvalidBind(bind.to_string()))?;
        Self::new(addr, database_url.to_owned())
    }

    /// Return the bind address.
    #[must_use]
    pub fn bind(&self) -> SocketAddr { self.bind }

    /// Return the configured database URL/file path.
    #[must_use]
    pub fn database_url(&self) -> &str { &self.database_url }

    /// Returns `true` when the configuration targets PostgreSQL.
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    fn uses_postgres(&self) -> bool { is_postgres_url(&self.database_url) }
}

/// Run the legacy Tokio server using the provided configuration.
///
/// # Errors
/// Propagates any network, database, or protocol errors encountered while
/// starting the listener or serving requests.
pub async fn run(cfg: &LegacyServerConfig) -> Result<()> {
    let pool = setup_database(cfg).await?;
    let listener = TcpListener::bind(cfg.bind()).await?;
    println!("mxd listening on {}", cfg.bind());
    accept_connections(listener, pool).await
}

async fn setup_database(cfg: &LegacyServerConfig) -> Result<DbPool> {
    let pool = create_pool(cfg).await;
    {
        let mut conn = pool.get().await.expect("failed to get db connection");
        #[cfg(feature = "sqlite")]
        audit_sqlite_features(&mut conn).await?;
        #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
        if cfg.uses_postgres() {
            audit_postgres_features(&mut conn).await?;
        }
        apply_migrations(&mut conn, cfg.database_url()).await?;
    }
    Ok(pool)
}

async fn create_pool(cfg: &LegacyServerConfig) -> DbPool {
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    if cfg.uses_postgres() {
        return establish_pool(cfg.database_url()).await;
    }
    establish_pool(cfg.database_url()).await
}

async fn accept_connections(listener: TcpListener, pool: DbPool) -> Result<()> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut join_set = JoinSet::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            () = &mut shutdown => {
                println!("shutdown signal received");
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((socket, peer)) => {
                        let pool = pool.clone();
                        let mut rx = shutdown_rx.clone();
                        join_set.spawn(async move {
                            if let Err(e) = handle_client(socket, peer, pool, &mut rx).await {
                                eprintln!("connection error from {peer}: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("accept error: {e}");
                    }
                }
            }
        }
    }

    // notify all tasks to shut down
    let _ = shutdown_tx.send(true);

    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res {
            eprintln!("task error: {e}");
        }
    }

    Ok(())
}

async fn handle_client(
    socket: TcpStream,
    peer: SocketAddr,
    pool: DbPool,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<()> {
    let (mut reader, mut writer) = tokio_io::split(socket);

    // perform protocol handshake with a timeout
    let mut buf = [0u8; protocol::HANDSHAKE_LEN];
    match timeout(protocol::HANDSHAKE_TIMEOUT, reader.read_exact(&mut buf)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                // Client disconnected before completing the handshake
                return Ok(());
            }
            return Err(e.into());
        }
        Err(_) => {
            protocol::write_handshake_reply(&mut writer, protocol::HANDSHAKE_ERR_TIMEOUT).await?;
            return Ok(());
        }
    }
    match protocol::parse_handshake(&buf) {
        Ok(_) => {
            protocol::write_handshake_reply(&mut writer, protocol::HANDSHAKE_OK).await?;
        }
        Err(err) => {
            let code = protocol::handshake_error_code(&err);
            protocol::write_handshake_reply(&mut writer, code).await?;
            return Ok(());
        }
    }

    let mut tx_reader = TransactionReader::new(reader);
    let mut tx_writer = TransactionWriter::new(writer);
    let ctx = HandlerContext::new(peer, pool.clone());
    let mut session = Session::default();
    loop {
        tokio::select! {
            tx = tx_reader.read_transaction() => match tx {
                Ok(tx) => {
                    let frame = tx.to_bytes();
                    let resp = handle_request(&ctx, &mut session, &frame)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                    tx_writer.write_transaction(&resp).await?;
                }
                Err(TransactionError::Io(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    // Remote closed the connection, end session gracefully
                    break;
                }
                Err(e) => return Err(e.into()),
            },
            _ = shutdown.changed() => {
                break;
            }
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    }
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
fn is_postgres_url(s: &str) -> bool {
    Url::parse(s)
        .map(|u| matches!(u.scheme(), "postgres" | "postgresql"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("127.0.0.1:5000", "mxd.db")]
    #[case("0.0.0.0:12345", "postgres://example.com/db")]
    fn builds_config_from_raw_success(#[case] bind: &str, #[case] db: &str) {
        let cfg = LegacyServerConfig::from_raw(bind, db).expect("config");
        assert_eq!(cfg.bind().to_string(), bind);
        assert_eq!(cfg.database_url(), db);
    }

    #[rstest]
    #[case("", "mxd.db", LegacyConfigError::InvalidBind(String::new()))]
    #[case("not-an-addr", "mxd.db", LegacyConfigError::InvalidBind("not-an-addr".into()))]
    #[case("127.0.0.1:1234", "   ", LegacyConfigError::EmptyDatabaseUrl)]
    fn rejects_invalid_config(
        #[case] bind: &str,
        #[case] db: &str,
        #[case] expected: LegacyConfigError,
    ) {
        let err = LegacyServerConfig::from_raw(bind, db).expect_err("should fail");
        assert_eq!(err, expected);
    }
}
