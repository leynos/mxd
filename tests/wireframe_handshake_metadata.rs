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
        connection::{HandshakeMetadata, registry_len, take_current_context},
        handshake,
        preamble::HotlinePreamble,
        test_helpers::preamble_bytes,
    },
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenarios, then, when};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    time::sleep,
};
use wireframe::{app::WireframeApp, server::WireframeServer};

const MAX_ATTEMPTS: usize = 50;
const POLL_INTERVAL_MS: u64 = 10;

struct MetadataWorld {
    addr: RefCell<Option<SocketAddr>>,
    shutdown: RefCell<Option<oneshot::Sender<()>>>,
    recorded: Arc<Mutex<Option<HandshakeMetadata>>>,
}

impl MetadataWorld {
    fn new() -> Self {
        Self {
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
        let recorded_state = Arc::clone(&self.recorded);
        let app_server = WireframeServer::new(move || {
            let handshake = take_current_context()
                .map(|context| context.handshake().clone())
                .unwrap_or_default();
            let recorded_factory = Arc::clone(&recorded_state);
            WireframeApp::<
                wireframe::serializer::BincodeSerializer,
                (),
                wireframe::app::Envelope,
            >::default()
            .app_data(handshake.clone())
            .on_connection_setup(move || {
                let recorded_setup = Arc::clone(&recorded_factory);
                let handshake_for_state = handshake.clone();
                async move {
                    recorded_setup
                        .lock()
                        .unwrap_or_else(|_| panic!("recorded handshake lock poisoned"))
                        .replace(handshake_for_state.clone());
                }
            })
            .unwrap_or_else(|err| panic!("failed to install connection setup: {err}"))
        })
        .workers(1)
        .with_preamble::<HotlinePreamble>();

        let handshake_server = handshake::install(app_server, Duration::from_millis(200));
        let bind_addr: SocketAddr = "127.0.0.1:0"
            .parse()
            .unwrap_or_else(|err| panic!("failed to parse bind address: {err}"));
        let bound_server = handshake_server
            .bind(bind_addr)
            .unwrap_or_else(|err| panic!("failed to bind server: {err}"));
        let addr = bound_server
            .local_addr()
            .unwrap_or_else(|| panic!("failed to read local address"));
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        tokio::spawn(async move {
            drop(
                bound_server
                    .run_with_shutdown(async {
                        let _shutdown_result = shutdown_rx.await;
                    })
                    .await,
            );
        });
        self.addr.borrow_mut().replace(addr);
        self.shutdown.borrow_mut().replace(shutdown_tx);
    }

    fn recorded(&self) -> Option<HandshakeMetadata> {
        self.recorded
            .lock()
            .unwrap_or_else(|_| panic!("recorded lock poisoned"))
            .clone()
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "test harness polling loop requires nested async block"
    )]
    async fn connect_and_send(&self, bytes: &[u8], expect_recorded: bool) {
        let Some(addr) = *self.addr.borrow() else {
            panic!("server not started");
        };
        // Capture the current recorded value to detect changes
        let previous = self.recorded();
        let mut stream = TcpStream::connect(addr)
            .await
            .unwrap_or_else(|err| panic!("failed to connect to test server: {err}"));
        stream
            .write_all(bytes)
            .await
            .unwrap_or_else(|err| panic!("failed to write handshake bytes: {err}"));
        let mut buf = [0u8; REPLY_LEN];
        drop(stream.read_exact(&mut buf).await);
        drop(stream);

        for _ in 0..MAX_ATTEMPTS {
            let current = self.recorded();
            // For expected recordings, wait for a DIFFERENT value (handles sequential calls)
            // For unexpected recordings, just check it's still None
            if expect_recorded {
                if current.is_some() && current != previous {
                    return;
                }
            } else if current.is_none() {
                return;
            }
            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }

        assert!(
            !expect_recorded,
            "handshake metadata was not recorded within the expected time"
        );
    }

    fn stop(&self) {
        if let Some(tx) = self.shutdown.borrow_mut().take() {
            let _send_result = tx.send(());
        }
    }
}

impl Drop for MetadataWorld {
    fn drop(&mut self) { self.stop(); }
}

#[fixture]
fn world() -> MetadataWorld {
    // Build a fresh test harness per scenario.
    MetadataWorld::new()
}

#[given("a wireframe server that records handshake metadata")]
async fn given_server(world: &MetadataWorld) { world.start_server(); }

#[when("I complete a Hotline handshake with sub-protocol \"{tag}\" and sub-version {sub_version}")]
async fn when_valid_handshake(world: &MetadataWorld, tag: String, sub_version: u16) {
    let mut sub_protocol = [0u8; 4];
    sub_protocol.copy_from_slice(tag.as_bytes());
    let bytes = preamble_bytes(*PROTOCOL_ID, sub_protocol, VERSION, sub_version);
    world.connect_and_send(&bytes, true).await;
}

#[when("I send a Hotline handshake with protocol \"{tag}\" and version {version}")]
async fn when_invalid_handshake(world: &MetadataWorld, tag: String, version: u16) {
    let mut protocol = [0u8; 4];
    protocol.copy_from_slice(tag.as_bytes());
    let bytes = preamble_bytes(protocol, *b"CHAT", version, 0);
    world.connect_and_send(&bytes, false).await;
}

#[then("the recorded handshake sub-protocol is \"{tag}\"")]
fn then_sub_protocol(world: &MetadataWorld, tag: String) {
    let Some(recorded) = world.recorded() else {
        panic!("handshake not recorded");
    };
    let mut expected = [0u8; 4];
    expected.copy_from_slice(tag.as_bytes());
    assert_eq!(recorded.sub_protocol_tag(), expected);
}

#[then("the recorded handshake sub-version is {sub_version}")]
fn then_sub_version(world: &MetadataWorld, sub_version: u16) {
    let Some(recorded) = world.recorded() else {
        panic!("handshake not recorded");
    };
    assert_eq!(recorded.sub_version, sub_version);
}

#[then("the handshake registry is cleared after teardown")]
fn then_registry_cleared(world: &MetadataWorld) {
    assert_eq!(registry_len(), 0, "handshake registry should be empty");
    // Reset captured metadata to prevent cross-scenario leakage when the fixture is reused.
    world
        .recorded
        .lock()
        .unwrap_or_else(|_| panic!("recorded lock poisoned"))
        .take();
}

#[then("no handshake metadata is recorded")]
fn then_no_metadata(world: &MetadataWorld) {
    assert!(world.recorded().is_none());
    assert_eq!(registry_len(), 0);
}

scenarios!(
    "tests/features/wireframe_handshake_metadata.feature",
    runtime = "tokio-current-thread",
    fixtures = [world: MetadataWorld]
);
