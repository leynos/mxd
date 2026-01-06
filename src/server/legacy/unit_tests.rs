//! Unit tests for legacy server helpers, ensuring internal behaviours remain
//! stable without requiring the external binary.

use std::sync::Arc;

use anyhow::Result;
use argon2::Argon2;
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use rstest::rstest;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};

use super::{ServerResources, handle_accept_result, test_helpers};
use crate::protocol;

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
#[rstest]
#[case("postgres://localhost", true)]
#[case("postgresql://localhost", true)]
#[case("sqlite://localhost", false)]
fn postgres_url_detection(#[case] url: &str, #[case] expected: bool) {
    assert_eq!(super::is_postgres_url(url), expected);
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[tokio::test]
async fn handle_accept_result_shares_argon2_between_clients() -> Result<()> {
    let pool = test_helpers::dummy_pool();
    let argon2 = Arc::new(Argon2::default());
    let strong_before = Arc::strong_count(&argon2);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut join_set = JoinSet::new();
    let resources = ServerResources {
        pool,
        argon2: Arc::clone(&argon2),
    };
    // resources holds one clone; count is now strong_before + 1
    let after_resources = Arc::strong_count(&argon2);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let mut client = TcpStream::connect(addr).await?;
    let (server_socket, peer) = listener.accept().await?;

    handle_accept_result(
        Ok((server_socket, peer)),
        &resources,
        &shutdown_rx,
        &mut join_set,
    );

    // Spawned task receives a clone of resources, adding one more reference.
    assert_eq!(Arc::strong_count(&argon2), after_resources + 1);

    client.write_all(&test_helpers::handshake_frame()).await?;
    let mut reply = [0u8; protocol::REPLY_LEN];
    client.read_exact(&mut reply).await?;
    client.shutdown().await?;

    shutdown_tx
        .send(true)
        .expect("shutdown receivers should remain until broadcast");

    while let Some(result) = join_set.join_next().await {
        result.expect("client handler task");
    }

    // After task completes, only the original and resources clone remain.
    assert_eq!(Arc::strong_count(&argon2), strong_before + 1);

    Ok(())
}
