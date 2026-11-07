//! BDD coverage for Wireframe handshake hooks.
//!
//! The world holds plain state and performs its network work through `async`
//! methods, so each scenario runs on the test's own Tokio runtime and the
//! steps read as ordinary sequential code.

use std::{net::SocketAddr, time::Duration};

use rstest::fixture;
use rstest_bdd::assert_step_ok;
use rstest_bdd_macros::{given, scenario, then, when};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    time::timeout,
};

use crate::{
    protocol::{PROTOCOL_ID, REPLY_LEN, VERSION},
    wireframe::test_helpers::preamble_bytes,
};

async fn perform_handshake(
    addr: SocketAddr,
    bytes: Option<Vec<u8>>,
) -> Result<[u8; REPLY_LEN], String> {
    let mut stream = TcpStream::connect(addr)
        .await
        .map_err(|err| err.to_string())?;
    send_handshake_bytes(&mut stream, bytes).await?;
    read_handshake_reply(&mut stream).await
}

async fn send_handshake_bytes(
    stream: &mut TcpStream,
    bytes: Option<Vec<u8>>,
) -> Result<(), String> {
    if let Some(data) = bytes {
        stream
            .write_all(&data)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn read_handshake_reply(stream: &mut TcpStream) -> Result<[u8; REPLY_LEN], String> {
    let mut buf = [0u8; REPLY_LEN];
    timeout(Duration::from_secs(1), stream.read_exact(&mut buf))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())?;
    Ok(buf)
}

/// Shared state for the handshake scenarios.
///
/// The server runs as a spawned task on the scenario's own Tokio runtime, so
/// the world only needs to remember where to reach it and how to stop it.
#[derive(Default)]
struct HandshakeWorld {
    addr: Option<SocketAddr>,
    shutdown: Option<oneshot::Sender<()>>,
    reply: Option<Result<[u8; REPLY_LEN], String>>,
}

impl HandshakeWorld {
    /// Start a handshake server with the short timeout the scenarios expect.
    ///
    /// The helper panics rather than returning an error if the socket cannot
    /// be bound, matching the rest of the wireframe test scaffolding. Spawning
    /// the server is synchronous, so only connecting and reading need to wait
    /// on I/O.
    fn start_server(&mut self) {
        let (addr, shutdown) = super::tests::start_server(Duration::from_millis(100));
        self.addr = Some(addr);
        self.shutdown = Some(shutdown);
    }

    /// Run one handshake against the started server, if any.
    ///
    /// The step succeeds whenever the attempt itself completed, whether the
    /// server answered with a success or an error code, so the `Then` step
    /// always has a reply to decode. Only a start-up failure (no server) is
    /// returned as an error. An outright client failure — connect, write, or
    /// read — is stored as `Err` and is not a reply, so `Then` reports it as a
    /// step failure rather than asserting a code for it.
    async fn connect_and_maybe_send(&mut self, bytes: Option<Vec<u8>>) -> Result<(), String> {
        let addr = self.addr.ok_or_else(|| "server not started".to_string())?;
        self.reply = Some(perform_handshake(addr, bytes).await);
        Ok(())
    }

    /// Decode the reply code from the bytes recorded by the handshake step.
    fn reply_code(&self) -> Result<u32, String> {
        let Some(reply) = self.reply.as_ref() else {
            return Err("missing reply".into());
        };
        reply
            .as_ref()
            .map(|buf| {
                let bytes = buf[4..8]
                    .try_into()
                    .map_err(|err| format!("convert reply slice to array (bdd reply): {err}"))?;
                Ok(u32::from_be_bytes(bytes))
            })
            .map_err(ToString::to_string)?
    }
}

impl Drop for HandshakeWorld {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

#[expect(
    clippy::allow_attributes,
    reason = "rustc compiler does not emit expected lint"
)]
#[allow(unused_braces, reason = "rstest-bdd macro expansion produces braces")]
#[fixture]
fn world() -> HandshakeWorld { HandshakeWorld::default() }

#[given("a wireframe server handling handshakes")]
fn given_server(world: &mut HandshakeWorld) { world.start_server(); }

#[when("I send a valid Hotline handshake")]
async fn when_valid(world: &mut HandshakeWorld) -> Result<(), String> {
    let bytes = preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 0);
    world.connect_and_maybe_send(Some(bytes.to_vec())).await
}

#[when("I send a Hotline handshake with protocol \"{tag}\" and version {version}")]
async fn when_custom(world: &mut HandshakeWorld, tag: String, version: u16) -> Result<(), String> {
    let mut protocol = [0u8; 4];
    if tag.len() != protocol.len() {
        return Err(format!(
            "protocol tag must be exactly {} bytes, got {}",
            protocol.len(),
            tag.len()
        ));
    }
    protocol.copy_from_slice(tag.as_bytes());
    let bytes = preamble_bytes(protocol, *b"CHAT", version, 0);
    world.connect_and_maybe_send(Some(bytes.to_vec())).await
}

#[when("I connect without sending a handshake")]
async fn when_idle(world: &mut HandshakeWorld) -> Result<(), String> {
    world.connect_and_maybe_send(None).await
}

#[then("the handshake reply code is {code}")]
fn then_code(world: &HandshakeWorld, code: u32) {
    let reply = world.reply_code();
    let value = assert_step_ok!(reply);
    assert_eq!(value, code);
}

#[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 0)]
#[tokio::test(flavor = "current_thread")]
async fn replies_ok(_world: HandshakeWorld) {}

#[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 1)]
#[tokio::test(flavor = "current_thread")]
async fn invalid_protocol(_world: HandshakeWorld) {}

#[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 2)]
#[tokio::test(flavor = "current_thread")]
async fn unsupported_version(_world: HandshakeWorld) {}

#[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 3)]
#[tokio::test(flavor = "current_thread")]
async fn handshake_timeout(_world: HandshakeWorld) {}

/// `Drop` must deliver the shutdown signal, not merely release it.
///
/// A dropped sender and a delivered `()` are different observations: the
/// receiver yields `Err(RecvError)` for the former and `Ok(())` for the
/// latter, so this assertion discriminates a real send from a silent release.
#[test]
fn dropping_the_world_signals_shutdown() {
    let (sender, mut receiver) = oneshot::channel();
    // Assign into a `Default` world rather than using functional-update
    // syntax: moving the remaining fields out of a base value is rejected for
    // a type that implements `Drop` (`E0509`).
    let mut world = HandshakeWorld::default();
    world.shutdown = Some(sender);

    drop(world);

    assert_eq!(receiver.try_recv(), Ok(()));
}
