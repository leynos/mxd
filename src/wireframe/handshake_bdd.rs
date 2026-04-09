//! BDD coverage for Wireframe handshake hooks.

use std::{cell::RefCell, net::SocketAddr, time::Duration};

use rstest::fixture;
use rstest_bdd::assert_step_ok;
use rstest_bdd_macros::{given, scenario, then, when};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    runtime::Runtime,
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
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    send_handshake_bytes(&mut stream, bytes).await;
    read_handshake_reply(&mut stream).await
}

async fn send_handshake_bytes(stream: &mut TcpStream, bytes: Option<Vec<u8>>) {
    if let Some(data) = bytes {
        stream.write_all(&data).await.expect("write handshake");
    }
}

async fn read_handshake_reply(stream: &mut TcpStream) -> Result<[u8; REPLY_LEN], String> {
    let mut buf = [0u8; REPLY_LEN];
    timeout(Duration::from_secs(1), stream.read_exact(&mut buf))
        .await
        .map(|res| {
            res.expect("read reply");
            buf
        })
        .map_err(|err| err.to_string())
}

struct HandshakeWorld {
    rt: Runtime,
    addr: RefCell<Option<SocketAddr>>,
    shutdown: RefCell<Option<oneshot::Sender<()>>>,
    reply: RefCell<Option<Result<[u8; REPLY_LEN], String>>>,
}

impl HandshakeWorld {
    fn new() -> Self {
        Self {
            rt: Runtime::new().expect("runtime"),
            addr: RefCell::new(None),
            shutdown: RefCell::new(None),
            reply: RefCell::new(None),
        }
    }

    fn start_server(&self) {
        let (addr, shutdown) = self
            .rt
            .block_on(async { super::tests::start_server(Duration::from_millis(100)) });
        self.addr.borrow_mut().replace(addr);
        self.shutdown.borrow_mut().replace(shutdown);
    }

    fn connect_and_maybe_send(&self, bytes: Option<Vec<u8>>) {
        let addr = self.addr.borrow().expect("server not started");
        let reply = self.rt.block_on(perform_handshake(addr, bytes));
        self.reply.borrow_mut().replace(reply);
    }

    fn reply_code(&self) -> Result<u32, String> {
        let reply = self.reply.borrow();
        let Some(reply) = reply.as_ref() else {
            return Err("missing reply".into());
        };
        reply
            .as_ref()
            .map(|buf| {
                u32::from_be_bytes(
                    buf[4..8]
                        .try_into()
                        .expect("convert reply slice to array (bdd reply)"),
                )
            })
            .map_err(ToString::to_string)
    }
}

impl Drop for HandshakeWorld {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.borrow_mut().take() {
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
fn world() -> HandshakeWorld { HandshakeWorld::new() }

#[given("a wireframe server handling handshakes")]
fn given_server(world: &HandshakeWorld) { world.start_server(); }

#[when("I send a valid Hotline handshake")]
fn when_valid(world: &HandshakeWorld) {
    let bytes = preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 0);
    world.connect_and_maybe_send(Some(bytes.to_vec()));
}

#[when("I send a Hotline handshake with protocol \"{tag}\" and version {version}")]
fn when_custom(world: &HandshakeWorld, tag: String, version: u16) {
    let mut protocol = [0u8; 4];
    protocol.copy_from_slice(tag.as_bytes());
    let bytes = preamble_bytes(protocol, *b"CHAT", version, 0);
    world.connect_and_maybe_send(Some(bytes.to_vec()));
}

#[when("I connect without sending a handshake")]
fn when_idle(world: &HandshakeWorld) { world.connect_and_maybe_send(None); }

#[then("the handshake reply code is {code}")]
fn then_code(world: &HandshakeWorld, code: u32) {
    let reply = world.reply_code();
    let value = assert_step_ok!(reply);
    assert_eq!(value, code);
}

#[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 0)]
fn replies_ok(world: HandshakeWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 1)]
fn invalid_protocol(world: HandshakeWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 2)]
fn unsupported_version(world: HandshakeWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 3)]
fn handshake_timeout(world: HandshakeWorld) { let _ = world; }
