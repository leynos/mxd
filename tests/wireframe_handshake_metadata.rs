//! Behavioural tests for persisting Hotline handshake metadata per connection.

use std::{
    cell::RefCell,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use mxd::{
    protocol::{PROTOCOL_ID, REPLY_LEN, VERSION},
    wireframe::{
        connection::{HandshakeMetadata, has_current_context, take_current_context},
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
    sync::{mpsc, oneshot},
    time::{sleep, timeout},
};
use wireframe::{app::WireframeApp, server::WireframeServer};

const MAX_ATTEMPTS: usize = 50;
const POLL_INTERVAL_MS: u64 = 10;

/// Test world for handshake-metadata scenarios.
///
/// Uses `RefCell` for non-shared fields; safe only under a single-threaded
/// Tokio runtime (`tokio-current-thread`).
struct MetadataWorld {
    addr: RefCell<Option<SocketAddr>>,
    shutdown: RefCell<Option<oneshot::Sender<()>>>,
    recorded: Arc<Mutex<Option<HandshakeMetadata>>>,
    teardown_sender: mpsc::UnboundedSender<()>,
    teardown_receiver: Arc<Mutex<mpsc::UnboundedReceiver<()>>>,
}

impl MetadataWorld {
    fn new() -> Self {
        let (teardown_sender, teardown_receiver) = mpsc::unbounded_channel();
        Self {
            addr: RefCell::new(None),
            shutdown: RefCell::new(None),
            recorded: Arc::new(Mutex::new(None)),
            teardown_sender,
            teardown_receiver: Arc::new(Mutex::new(teardown_receiver)),
        }
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "wireframe server setup requires nested closures for app factory and shutdown"
    )]
    fn start_server(&self) {
        let recorded_state = Arc::clone(&self.recorded);
        let teardown_state = self.teardown_sender.clone();
        let app_server = WireframeServer::new(move || {
            let handshake = take_current_context()
                .map(|context| context.handshake().clone())
                .unwrap_or_default();
            let recorded_factory = Arc::clone(&recorded_state);
            let teardown_factory = teardown_state.clone();
            let app_result = WireframeApp::<
                wireframe::serializer::BincodeSerializer,
                (),
                wireframe::app::Envelope,
            >::default()
            .app_data(handshake.clone())
            .on_connection_setup(move || {
                let recorded_setup = Arc::clone(&recorded_factory);
                let handshake_for_state = handshake.clone();
                async move {
                    match recorded_setup.lock() {
                        Ok(mut recorded) => {
                            recorded.replace(handshake_for_state.clone());
                        }
                        Err(poisoned) => {
                            panic!("recorded handshake lock poisoned: {poisoned}");
                        }
                    }
                }
            })
            .and_then(|app| {
                app.on_connection_teardown(move |(): ()| {
                    let teardown_setup = teardown_factory.clone();
                    async move {
                        let _send_result = teardown_setup.send(());
                    }
                })
            });
            match app_result {
                Ok(app) => app,
                Err(err) => panic!("failed to install connection setup: {err}"),
            }
        })
        .workers(1)
        .with_preamble::<HotlinePreamble>();

        let handshake_server = handshake::install(app_server, Duration::from_millis(200));
        let bind_addr: SocketAddr = match "127.0.0.1:0".parse() {
            Ok(bind_addr) => bind_addr,
            Err(err) => panic!("failed to parse bind address: {err}"),
        };
        let bound_server = match handshake_server.bind(bind_addr) {
            Ok(bound_server) => bound_server,
            Err(err) => panic!("failed to bind server: {err}"),
        };
        let Some(addr) = bound_server.local_addr() else {
            panic!("failed to read local address");
        };
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
        self.recorded.lock().map_or_else(
            |_poisoned| panic!("recorded lock poisoned"),
            |recorded| recorded.clone(),
        )
    }

    async fn connect_and_send(
        &self,
        bytes: &[u8],
        expect_recorded: bool,
    ) -> Result<(), anyhow::Error> {
        self.clear_recorded();
        self.drain_teardown_signals();
        let addr = self.server_addr();
        self.open_and_write(addr, bytes).await?;
        let recorded = self.await_recorded().await?;
        Self::assert_recording(expect_recorded, recorded.as_ref())?;
        Ok(())
    }

    fn server_addr(&self) -> SocketAddr {
        let Some(addr) = *self.addr.borrow() else {
            panic!("server not started");
        };
        addr
    }

    async fn open_and_write(&self, addr: SocketAddr, bytes: &[u8]) -> Result<(), anyhow::Error> {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|err| anyhow::anyhow!("failed to connect to test server: {err}"))?;
        stream
            .write_all(bytes)
            .await
            .map_err(|err| anyhow::anyhow!("failed to write handshake bytes: {err}"))?;
        let mut buf = [0u8; REPLY_LEN];
        stream
            .read_exact(&mut buf)
            .await
            .map_err(|err| anyhow::anyhow!("failed to read handshake reply: {err}"))?;
        drop(stream);
        Ok(())
    }

    async fn await_recorded(&self) -> Result<Option<HandshakeMetadata>, anyhow::Error> {
        for attempt in 0..MAX_ATTEMPTS {
            let current = self.recorded();
            if current.is_some() {
                return Ok(current);
            } else if attempt + 1 == MAX_ATTEMPTS {
                return Ok(None);
            }
            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }

        Ok(None)
    }

    fn clear_recorded(&self) {
        match self.recorded.lock() {
            Ok(mut recorded) => {
                recorded.take();
            }
            Err(poisoned) => panic!("recorded lock poisoned: {poisoned}"),
        }
    }

    fn drain_teardown_signals(&self) {
        match self.teardown_receiver.lock() {
            Ok(mut receiver) => while receiver.try_recv().is_ok() {},
            Err(poisoned) => panic!("teardown receiver lock poisoned: {poisoned}"),
        }
    }

    async fn wait_for_teardown(&self) -> Result<(), anyhow::Error> {
        timeout(Duration::from_secs(1), self.wait_for_teardown_signal())
            .await
            .context("timed out waiting for connection teardown signal")?
    }

    async fn wait_for_teardown_signal(&self) -> Result<(), anyhow::Error> {
        loop {
            if self.poll_teardown_signal()? {
                return Ok(());
            }
            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    }

    fn poll_teardown_signal(&self) -> Result<bool, anyhow::Error> {
        let mut receiver = self
            .teardown_receiver
            .lock()
            .map_err(|poisoned| anyhow::anyhow!("teardown receiver lock poisoned: {poisoned}"))?;
        match receiver.try_recv() {
            Ok(()) => Ok(true),
            Err(mpsc::error::TryRecvError::Empty) => Ok(false),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                anyhow::bail!("connection teardown signal sender was dropped");
            }
        }
    }

    fn assert_recording(
        expect_recorded: bool,
        recorded: Option<&HandshakeMetadata>,
    ) -> Result<(), anyhow::Error> {
        if expect_recorded {
            anyhow::ensure!(
                recorded.is_some(),
                "handshake metadata was not recorded within the expected time"
            );
        } else {
            anyhow::ensure!(
                recorded.is_none(),
                "handshake metadata was recorded unexpectedly"
            );
        }
        Ok(())
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
async fn given_server(world: &MetadataWorld) {
    // The async step signature matches scenario runtime requirements; setup is synchronous.
    world.start_server();
}

async fn run_handshake_step(
    world: &MetadataWorld,
    tag: &str,
    version: u16,
    expect_recorded: bool,
) -> anyhow::Result<()> {
    let mut protocol_tag = [0u8; 4];
    protocol_tag.copy_from_slice(tag.as_bytes());
    let bytes = if expect_recorded {
        preamble_bytes(*PROTOCOL_ID, protocol_tag, VERSION, version)
    } else {
        preamble_bytes(protocol_tag, *b"CHAT", version, 0)
    };
    world
        .connect_and_send(&bytes, expect_recorded)
        .await
        .with_context(|| format!("failed to run handshake step for tag {tag}"))
}

#[when("I complete a Hotline handshake with sub-protocol \"{tag}\" and sub-version {sub_version}")]
async fn when_valid_handshake(
    world: &MetadataWorld,
    tag: String,
    sub_version: u16,
) -> Result<(), anyhow::Error> {
    run_handshake_step(world, &tag, sub_version, true).await
}

#[when("I send a Hotline handshake with protocol \"{tag}\" and version {version}")]
async fn when_invalid_handshake(
    world: &MetadataWorld,
    tag: String,
    version: u16,
) -> Result<(), anyhow::Error> {
    run_handshake_step(world, &tag, version, false).await
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
async fn then_registry_cleared(world: &MetadataWorld) -> Result<(), anyhow::Error> {
    world.wait_for_teardown().await
}

#[then("no handshake metadata is recorded")]
fn then_no_metadata(world: &MetadataWorld) {
    assert!(world.recorded().is_none());
    assert!(!has_current_context());
}

scenarios!(
    "tests/features/wireframe_handshake_metadata.feature",
    runtime = "tokio-current-thread",
    fixtures = [world: MetadataWorld]
);
