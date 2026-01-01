//! Wireframe-based server runtime.
//!
//! This module bootstraps the Hotline protocol server using a custom TCP accept
//! loop with Hotline-specific frame handling. The wireframe library is used only
//! for preamble (handshake) handling, while the connection handler uses
//! [`HotlineCodec`] for proper 20-byte header framing.
//!
//! # Architecture
//!
//! The bootstrap process:
//!
//! 1. Establishes a database connection pool
//! 2. Creates a shared Argon2 instance for password hashing
//! 3. Binds to the configured address
//! 4. Accepts connections and handles preamble (handshake) using wireframe
//! 5. Routes transactions through [`connection_handler`] with Hotline framing
//!
//! This implementation fulfils the roadmap task "Route transactions through
//! wireframe" by using custom frame handling as described in
//! `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`.
//!
//! [`HotlineCodec`]: crate::wireframe::codec::HotlineCodec
//! [`connection_handler`]: crate::wireframe::connection_handler

#![expect(
    clippy::shadow_reuse,
    reason = "intentional shadowing for server building"
)]
#![expect(
    clippy::print_stdout,
    reason = "intentional console output for server status"
)]

#[cfg(test)]
use std::sync::{Mutex, OnceLock};
use std::{
    io::{self, Write},
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use tokio::{net::TcpListener, sync::Mutex as TokioMutex, time::timeout};
use tracing::{error, info, warn};
use wireframe::{preamble::read_preamble, rewind_stream::RewindStream};

use super::{AppConfig, Cli};
#[cfg(test)]
use crate::wireframe::connection::HandshakeMetadata;
use crate::{
    db::{DbPool, establish_pool},
    handler::Session,
    protocol::{
        self,
        HANDSHAKE_ERR_INVALID,
        HANDSHAKE_ERR_TIMEOUT,
        HANDSHAKE_ERR_UNSUPPORTED_VERSION,
        HANDSHAKE_INVALID_PROTOCOL_TOKEN,
        HANDSHAKE_OK,
        HANDSHAKE_UNSUPPORTED_VERSION_TOKEN,
        write_handshake_reply,
    },
    server::admin,
    wireframe::{connection_handler, preamble::HotlinePreamble},
};

#[cfg(test)]
static LAST_HANDSHAKE: OnceLock<Mutex<Option<HandshakeMetadata>>> = OnceLock::new();

/// Parse CLI arguments and start the Wireframe runtime.
///
/// # Errors
///
/// Propagates failures from configuration loading or the Wireframe runtime.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    run_with_cli(cli).await
}

/// Execute the Wireframe runtime with a pre-parsed [`Cli`].
///
/// # Errors
///
/// Returns any error raised while running administrative commands or binding
/// the Wireframe listener.
pub async fn run_with_cli(cli: Cli) -> Result<()> {
    let Cli { config, command } = cli;
    if let Some(command) = command {
        admin::run_command(command, &config).await
    } else {
        run_daemon(config).await
    }
}

async fn run_daemon(config: AppConfig) -> Result<()> {
    let bootstrap = WireframeBootstrap::prepare(config)?;
    bootstrap.run().await
}

#[derive(Clone, Debug)]
struct WireframeBootstrap {
    bind_addr: SocketAddr,
    config: Arc<AppConfig>,
}

impl WireframeBootstrap {
    fn prepare(config: AppConfig) -> Result<Self> {
        let bind_addr = parse_bind_addr(&config.bind)?;
        Ok(Self {
            bind_addr,
            config: Arc::new(config),
        })
    }

    #[expect(
        clippy::cognitive_complexity,
        reason = "server bootstrap has inherent complexity from multiple initialization steps"
    )]
    async fn run(self) -> Result<()> {
        let Self { bind_addr, config } = self;
        println!("mxd-wireframe-server using database {}", config.database);
        println!("mxd-wireframe-server binding to {}", config.bind);

        // Establish the database connection pool
        let pool = establish_pool(&config.database)
            .await
            .context("failed to establish database pool")?;

        // Bind the TCP listener
        let listener = TcpListener::bind(bind_addr)
            .await
            .context("failed to bind TCP listener")?;
        let addr = listener
            .local_addr()
            .context("failed to get local address")?;

        println!("mxd-wireframe-server listening on {addr}");
        // Explicit flush ensures the message reaches piped stdout immediately,
        // which is critical for test harness readiness detection.
        std::io::stdout().flush().ok();

        // Accept loop
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    let pool = pool.clone();
                    tokio::spawn(spawn_connection_task(stream, peer, pool));
                }
                Err(e) => {
                    warn!(error = %e, "accept failed");
                    // Continue accepting; transient failures shouldn't stop the server
                }
            }
        }
    }
}

/// Spawn a connection handler task, logging any errors.
async fn spawn_connection_task(stream: tokio::net::TcpStream, peer: SocketAddr, pool: DbPool) {
    if let Err(e) = handle_client(stream, peer, pool).await {
        error!(peer = %peer, error = %e, "connection handler failed");
    }
}

/// Handle a single client connection.
///
/// This function:
/// 1. Reads and validates the preamble (handshake)
/// 2. Sends the handshake reply
/// 3. Processes transactions using the custom connection handler
#[expect(
    clippy::cognitive_complexity,
    reason = "connection handling with error branching has inherent complexity"
)]
async fn handle_client(
    mut stream: tokio::net::TcpStream,
    peer: SocketAddr,
    pool: DbPool,
) -> Result<()> {
    info!(peer = %peer, "accepted connection");

    // Read preamble with timeout
    let preamble_result = timeout(
        protocol::HANDSHAKE_TIMEOUT,
        read_preamble::<_, HotlinePreamble>(&mut stream),
    )
    .await;

    let (preamble, leftover) = match preamble_result {
        Ok(Ok((preamble, leftover))) => (preamble, leftover),
        Ok(Err(e)) => {
            // Decode error - determine error code
            let code = error_code_for_decode(&e);
            if let Some(code) = code {
                write_handshake_reply(&mut stream, code).await.ok();
            }
            return Err(anyhow!("preamble decode failed: {e}"));
        }
        Err(_) => {
            // Timeout
            write_handshake_reply(&mut stream, HANDSHAKE_ERR_TIMEOUT)
                .await
                .ok();
            return Err(anyhow!("preamble read timed out"));
        }
    };

    // Store handshake metadata for test assertions
    #[cfg(test)]
    {
        let handshake_meta = HandshakeMetadata::from(preamble.handshake());
        let last = LAST_HANDSHAKE.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = last.lock() {
            guard.replace(handshake_meta);
        }
    }

    // Suppress unused variable warning in non-test builds
    #[cfg(not(test))]
    let _ = preamble;

    // Send success reply
    write_handshake_reply(&mut stream, HANDSHAKE_OK)
        .await
        .map_err(|e| anyhow!("failed to send handshake reply: {e}"))?;

    info!(peer = %peer, "handshake complete");

    // Wrap stream with leftover bytes and handle transactions
    let rewind_stream = RewindStream::new(leftover, stream);
    let session = Arc::new(TokioMutex::new(Session::default()));

    connection_handler::handle_connection(rewind_stream, peer, pool, session)
        .await
        .map_err(|e| anyhow!("connection handler error: {e}"))
}

/// Determine the Hotline error code for a decode error.
fn error_code_for_decode(err: &bincode::error::DecodeError) -> Option<u32> {
    use bincode::error::DecodeError;

    match err {
        DecodeError::OtherString(text) => error_code_from_str(text),
        DecodeError::Other(text) => error_code_from_str(text),
        DecodeError::Io { inner, .. } if inner.kind() == io::ErrorKind::TimedOut => {
            Some(HANDSHAKE_ERR_TIMEOUT)
        }
        _ => None,
    }
}

/// Check error message for known Hotline error tokens.
fn error_code_from_str(text: &str) -> Option<u32> {
    if text.starts_with(HANDSHAKE_INVALID_PROTOCOL_TOKEN) {
        Some(HANDSHAKE_ERR_INVALID)
    } else if text.starts_with(HANDSHAKE_UNSUPPORTED_VERSION_TOKEN) {
        Some(HANDSHAKE_ERR_UNSUPPORTED_VERSION)
    } else {
        None
    }
}

fn parse_bind_addr(target: &str) -> Result<SocketAddr> {
    target
        .parse()
        .or_else(|_| resolve_hostname(target))
        .with_context(|| format!("invalid bind address '{target}'"))
}

fn resolve_hostname(target: &str) -> Result<SocketAddr> {
    let mut addrs = target
        .to_socket_addrs()
        .with_context(|| format!("failed to resolve '{target}'"))?;
    addrs
        .next()
        .ok_or_else(|| anyhow!("failed to resolve '{target}'"))
}

#[cfg(test)]
mod tests {
    use rstest::{fixture, rstest};

    use super::*;

    #[fixture]
    fn bound_config() -> AppConfig {
        AppConfig {
            bind: "127.0.0.1:7777".to_string(),
            ..AppConfig::default()
        }
    }

    #[rstest]
    #[case("127.0.0.1:6000")]
    #[case("[::1]:7000")]
    fn parses_socket_addrs(#[case] bind: &str) {
        let addr = parse_bind_addr(bind).expect("bind");
        assert_eq!(addr.to_string(), bind);
    }

    #[rstest]
    #[case("invalid")]
    #[case("127.0.0.1")]
    fn rejects_invalid_addrs(#[case] bind: &str) {
        let err = parse_bind_addr(bind).expect_err("must fail");
        assert!(err.to_string().contains("invalid bind address"));
    }

    #[rstest]
    fn resolves_hostnames() {
        let addr = parse_bind_addr("localhost:6010").expect("bind");
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), 6010);
    }

    #[rstest]
    fn bootstrap_captures_bind(bound_config: AppConfig) {
        let bootstrap = WireframeBootstrap::prepare(bound_config).expect("bootstrap");
        assert_eq!(bootstrap.bind_addr, "127.0.0.1:7777".parse().unwrap());
        assert_eq!(bootstrap.config.bind, "127.0.0.1:7777");
    }
}

#[cfg(test)]
mod bdd {
    use std::cell::RefCell;

    use rstest::fixture;
    use rstest_bdd::{assert_step_err, assert_step_ok};
    use rstest_bdd_macros::{given, scenario, then, when};

    use super::*;

    struct BootstrapWorld {
        config: RefCell<AppConfig>,
        outcome: RefCell<Option<Result<WireframeBootstrap>>>,
    }

    impl BootstrapWorld {
        fn new() -> Self {
            Self {
                config: RefCell::new(AppConfig::default()),
                outcome: RefCell::new(None),
            }
        }

        fn set_bind(&self, bind: String) { self.config.borrow_mut().bind = bind; }

        fn bootstrap(&self) {
            let cfg = self.config.borrow().clone();
            let result = WireframeBootstrap::prepare(cfg);
            self.outcome.borrow_mut().replace(result);
        }
    }

    #[fixture]
    fn world() -> BootstrapWorld {
        let world = BootstrapWorld::new();
        world.config.borrow_mut().bind = "127.0.0.1:0".to_string();
        world
    }

    #[given("a wireframe configuration binding to \"{bind}\"")]
    fn given_bind(world: &BootstrapWorld, bind: String) { world.set_bind(bind); }

    #[when("I bootstrap the wireframe server")]
    fn when_bootstrap(world: &BootstrapWorld) { world.bootstrap(); }

    #[then("bootstrap succeeds")]
    fn then_success(world: &BootstrapWorld) {
        let outcome_ref = world.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("bootstrap not executed");
        };
        assert_step_ok!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
    }

    #[then("the resolved bind address is \"{bind}\"")]
    fn then_matches_bind(world: &BootstrapWorld, bind: String) {
        let outcome_ref = world.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("bootstrap not executed");
        };
        let bootstrap = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
        assert_eq!(bootstrap.bind_addr.to_string(), bind);
        drop(bind);
    }

    #[then("bootstrap fails with message \"{message}\"")]
    fn then_failure(world: &BootstrapWorld, message: String) {
        let outcome_ref = world.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("bootstrap not executed");
        };
        let text = assert_step_err!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
        assert!(
            text.contains(&message),
            "expected '{text}' to contain '{message}'"
        );
        drop(message);
    }

    #[scenario(path = "tests/features/wireframe_server.feature", index = 0)]
    fn accepts_bind(world: BootstrapWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_server.feature", index = 1)]
    fn rejects_bind(world: BootstrapWorld) { let _ = world; }
}
