#![allow(
    unfulfilled_lint_expectations,
    reason = "test lint expectations may not all trigger"
)]
#![expect(missing_docs, reason = "test file")]
#![expect(clippy::expect_used, reason = "test assertions")]
#![expect(clippy::unwrap_used, reason = "test assertions")]
#![expect(clippy::panic_in_result_fn, reason = "test assertions")]
#![expect(clippy::big_endian_bytes, reason = "network protocol")]
#![expect(clippy::let_underscore_must_use, reason = "test cleanup")]
#![expect(clippy::shadow_reuse, reason = "test code")]

//! Behavioural tests for persisting Hotline handshake metadata per connection.

use std::{
    cell::RefCell,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use mxd::{
    protocol::{PROTOCOL_ID, REPLY_LEN, VERSION},
    wireframe::{
        connection::{HandshakeMetadata, clear_current_handshake, current_handshake, registry_len},
        handshake,
        preamble::HotlinePreamble,
        test_helpers::preamble_bytes,
    },
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    runtime::Runtime,
    sync::oneshot,
    time::sleep,
};
use wireframe::{app::WireframeApp, server::WireframeServer};

const MAX_ATTEMPTS: usize = 50;
const POLL_INTERVAL_MS: u64 = 10;

struct MetadataWorld {
    rt: Runtime,
    addr: RefCell<Option<SocketAddr>>,
    shutdown: RefCell<Option<oneshot::Sender<()>>>,
    recorded: Arc<Mutex<Option<HandshakeMetadata>>>,
}

impl MetadataWorld {
    #[expect(
        clippy::expect_used,
        reason = "test harness should fail fast if the runtime cannot start"
    )]
    fn new() -> Self {
        Self {
            rt: Runtime::new().expect("runtime"),
            addr: RefCell::new(None),
            shutdown: RefCell::new(None),
            recorded: Arc::new(Mutex::new(None)),
        }
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "wireframe server setup requires nested closures for app factory and shutdown"
    )]
    fn start_server(&self) {
        let recorded = Arc::clone(&self.recorded);
        let (addr, shutdown_tx) = self.rt.block_on(async move {
            let server = WireframeServer::new(move || {
                let handshake = current_handshake().unwrap_or_default();
                let recorded = Arc::clone(&recorded);
                let app = WireframeApp::<
                    wireframe::serializer::BincodeSerializer,
                    (),
                    wireframe::app::Envelope,
                >::default()
                .app_data(handshake.clone())
                .on_connection_setup(move || {
                    let recorded = Arc::clone(&recorded);
                    let handshake_for_state = handshake.clone();
                    async move {
                        recorded
                            .lock()
                            .expect("recorded handshake lock")
                            .replace(handshake_for_state.clone());
                        clear_current_handshake();
                    }
                })
                .expect("install connection setup");
                clear_current_handshake();
                app
            })
            .workers(1)
            .with_preamble::<HotlinePreamble>();

            let server = handshake::install(server, Duration::from_millis(200));
            let server = server
                .bind("127.0.0.1:0".parse().expect("bind addr"))
                .expect("bind server");
            let addr = server.local_addr().expect("local addr");
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            tokio::spawn(async move {
                let _ = server
                    .run_with_shutdown(async {
                        let _ = shutdown_rx.await;
                    })
                    .await;
            });
            (addr, shutdown_tx)
        });
        self.addr.borrow_mut().replace(addr);
        self.shutdown.borrow_mut().replace(shutdown_tx);
    }

    fn recorded(&self) -> Option<HandshakeMetadata> {
        self.recorded.lock().expect("recorded lock").clone()
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "test harness polling loop requires nested async block"
    )]
    fn connect_and_send(&self, bytes: &[u8], expect_recorded: bool) {
        let addr = self.addr.borrow().expect("server not started");
        self.rt.block_on(async {
            let mut stream = TcpStream::connect(addr).await.expect("connect");
            stream.write_all(bytes).await.expect("write handshake");
            let mut buf = [0u8; REPLY_LEN];
            let _ = stream.read_exact(&mut buf).await;
            drop(stream);

            for _ in 0..MAX_ATTEMPTS {
                if self.recorded().is_some() || !expect_recorded {
                    return;
                }
                sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
            }

            panic!("handshake metadata was not recorded within the expected time");
        });
    }

    fn stop(&self) {
        if let Some(tx) = self.shutdown.borrow_mut().take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for MetadataWorld {
    fn drop(&mut self) { self.stop(); }
}

#[fixture]
fn world() -> MetadataWorld {
    // Build a fresh runtime-backed test harness per scenario.
    MetadataWorld::new()
}

#[given("a wireframe server that records handshake metadata")]
fn given_server(world: &MetadataWorld) { world.start_server(); }

#[when("I complete a Hotline handshake with sub-protocol \"{tag}\" and sub-version {sub_version}")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "rstest-bdd step parameters must currently own their captured strings"
)]
fn when_valid_handshake(world: &MetadataWorld, tag: String, sub_version: u16) {
    let mut sub_protocol = [0u8; 4];
    sub_protocol.copy_from_slice(tag.as_bytes());
    let bytes = preamble_bytes(*PROTOCOL_ID, sub_protocol, VERSION, sub_version);
    world.connect_and_send(&bytes, true);
}

#[when("I send a Hotline handshake with protocol \"{tag}\" and version {version}")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "rstest-bdd step parameters must currently own their captured strings"
)]
fn when_invalid_handshake(world: &MetadataWorld, tag: String, version: u16) {
    let mut protocol = [0u8; 4];
    protocol.copy_from_slice(tag.as_bytes());
    let bytes = preamble_bytes(protocol, *b"CHAT", version, 0);
    world.connect_and_send(&bytes, false);
}

#[then("the recorded handshake sub-protocol is \"{tag}\"")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "rstest-bdd step parameters must currently own their captured strings"
)]
fn then_sub_protocol(world: &MetadataWorld, tag: String) {
    let recorded = world.recorded().expect("handshake not recorded");
    let mut expected = [0u8; 4];
    expected.copy_from_slice(tag.as_bytes());
    assert_eq!(recorded.sub_protocol_tag(), expected);
}

#[then("the recorded handshake sub-version is {sub_version}")]
fn then_sub_version(world: &MetadataWorld, sub_version: u16) {
    let recorded = world.recorded().expect("handshake not recorded");
    assert_eq!(recorded.sub_version, sub_version);
}

#[then("the handshake registry is cleared after teardown")]
fn then_registry_cleared(world: &MetadataWorld) {
    assert_eq!(registry_len(), 0, "handshake registry should be empty");
    // Reset captured metadata to prevent cross-scenario leakage when the fixture is reused.
    world.recorded.lock().expect("recorded lock").take();
}

#[then("no handshake metadata is recorded")]
fn then_no_metadata(world: &MetadataWorld) {
    assert!(world.recorded().is_none());
    assert_eq!(registry_len(), 0);
}

#[scenario(
    path = "tests/features/wireframe_handshake_metadata.feature",
    index = 0
)]
fn records_metadata(world: MetadataWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_handshake_metadata.feature",
    index = 1
)]
fn rejects_invalid(world: MetadataWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_handshake_metadata.feature",
    index = 2
)]
fn metadata_does_not_leak(world: MetadataWorld) { let _ = world; }
