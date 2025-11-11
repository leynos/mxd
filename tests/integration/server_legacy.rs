//! Integration tests for the legacy Tokio server helpers.

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use argon2::Argon2;
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, bb8::Pool};
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use mxd::server::legacy::is_postgres_url;
use mxd::{
    db::{DbConnection, DbPool},
    protocol,
    server::legacy::handle_accept_result,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
#[test]
fn postgres_url_detection() {
    assert!(is_postgres_url("postgres://localhost"));
    assert!(is_postgres_url("postgresql://localhost"));
    assert!(!is_postgres_url("sqlite://localhost"));
}

fn dummy_pool() -> DbPool {
    let manager =
        AsyncDieselConnectionManager::<DbConnection>::new("postgres://example.invalid/mxd-test");
    Pool::builder()
        .max_size(1)
        .min_idle(Some(0))
        .idle_timeout(None::<Duration>)
        .max_lifetime(None::<Duration>)
        .test_on_check_out(false)
        .build_unchecked(manager)
}

fn handshake_frame() -> [u8; protocol::HANDSHAKE_LEN] {
    let mut buf = [0u8; protocol::HANDSHAKE_LEN];
    buf[0..4].copy_from_slice(protocol::PROTOCOL_ID);
    buf[8..10].copy_from_slice(&protocol::VERSION.to_be_bytes());
    buf
}

#[tokio::test]
async fn handle_accept_result_shares_argon2_between_clients() -> Result<()> {
    let pool = dummy_pool();
    let argon2 = Arc::new(Argon2::default());
    let strong_before = Arc::strong_count(&argon2);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut join_set = JoinSet::new();

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let mut client = TcpStream::connect(addr).await?;
    let (server_socket, peer) = listener.accept().await?;

    handle_accept_result(
        Ok((server_socket, peer)),
        pool,
        Arc::clone(&argon2),
        &shutdown_rx,
        &mut join_set,
    );

    assert_eq!(Arc::strong_count(&argon2), strong_before + 1);

    client.write_all(&handshake_frame()).await?;
    let mut reply = [0u8; protocol::REPLY_LEN];
    client.read_exact(&mut reply).await?;
    client.shutdown().await?;

    shutdown_tx.send(true).ok();

    while let Some(result) = join_set.join_next().await {
        result.expect("client handler task");
    }

    assert_eq!(Arc::strong_count(&argon2), strong_before);

    Ok(())
}
