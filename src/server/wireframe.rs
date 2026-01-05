//! Wireframe-based server runtime.
//!
//! This module bootstraps the Hotline protocol server using wireframe's
//! connection handling with a custom `HotlineFrameCodec` for the 20-byte
//! header framing. Routing is handled by middleware that dispatches Hotline
//! transactions to domain commands.
//!
//! # Architecture
//!
//! The bootstrap process:
//!
//! 1. Establishes a database connection pool
//! 2. Creates a shared Argon2 instance for password hashing
//! 3. Builds a `WireframeServer` with Hotline preamble hooks
//! 4. Registers the Hotline frame codec and routes
//! 5. Binds and runs the server
//!
//! [`HotlineFrameCodec`]: crate::wireframe::codec::HotlineFrameCodec

#![expect(
    clippy::shadow_reuse,
    reason = "intentional shadowing for server building"
)]
#![expect(
    clippy::print_stdout,
    reason = "intentional console output for server status"
)]

use std::{
    io::{self, Write},
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use argon2::Argon2;
use clap::Parser;
use tokio::sync::Mutex as TokioMutex;
use tracing::{error, warn};
use wireframe::{
    app::{Envelope, Handler, WireframeApp},
    serializer::BincodeSerializer,
    server::WireframeServer,
};

use super::{AppConfig, Cli};
use crate::{
    db::{DbPool, establish_pool},
    handler::Session,
    protocol,
    server::admin,
    wireframe::{
        codec::HotlineFrameCodec,
        connection::{HandshakeMetadata, take_current_context},
        handshake,
        preamble::HotlinePreamble,
        protocol::HotlineProtocol,
        route_ids::{FALLBACK_ROUTE_ID, ROUTE_IDS},
        routes::{RouteState, TransactionMiddleware},
    },
};

type HotlineApp = WireframeApp<BincodeSerializer, (), Envelope, HotlineFrameCodec>;

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

    async fn run(self) -> Result<()> {
        let Self { bind_addr, config } = self;
        println!("mxd-wireframe-server using database {}", config.database);
        println!("mxd-wireframe-server binding to {}", config.bind);

        let pool = establish_pool(&config.database)
            .await
            .context("failed to establish database pool")?;
        let argon2 = Arc::new(admin::argon2_from_config(&config)?);

        let app_factory = {
            let pool = pool.clone();
            let argon2 = Arc::clone(&argon2);
            move || build_app_for_connection(pool.clone(), Arc::clone(&argon2))
        };

        let server = WireframeServer::new(app_factory).with_preamble::<HotlinePreamble>();
        let server = handshake::install(server, protocol::HANDSHAKE_TIMEOUT);
        let server = server
            .bind(bind_addr)
            .context("failed to bind wireframe server")?;
        let addr = server
            .local_addr()
            .ok_or_else(|| anyhow!("failed to get local address"))?;

        announce_listening(addr);

        server.run().await.context("wireframe server terminated")?;
        Ok(())
    }
}

fn announce_listening(addr: SocketAddr) {
    println!("mxd-wireframe-server listening on {addr}");
    // Explicit flush ensures the message reaches piped stdout immediately,
    // which is critical for test harness readiness detection.
    if let Err(error) = io::stdout().flush() {
        warn!(%error, "failed to flush stdout");
    }
}

fn build_app_for_connection(pool: DbPool, argon2: Arc<Argon2<'static>>) -> HotlineApp {
    // Missing connection context indicates handshake setup failed; abort the
    // connection rather than running without routing state.
    let context = take_current_context().unwrap_or_else(|| {
        error!("missing handshake context in app factory");
        panic!("missing handshake context in app factory");
    });
    let (handshake, peer) = context.into_parts();
    let peer = peer.unwrap_or_else(|| {
        error!("peer address missing in app factory");
        panic!("peer address missing in app factory");
    });
    build_app(pool, argon2, handshake, peer).unwrap_or_else(|err| {
        error!(error = %err, "failed to build wireframe application");
        panic!("failed to build wireframe application: {err}");
    })
}

fn build_app(
    pool: DbPool,
    argon2: Arc<Argon2<'static>>,
    handshake: HandshakeMetadata,
    peer: SocketAddr,
) -> wireframe::app::Result<HotlineApp> {
    let session = Arc::new(TokioMutex::new(Session::default()));
    let protocol = HotlineProtocol::new(pool.clone(), Arc::clone(&argon2));

    let app = HotlineApp::default()
        .fragmentation(None)
        .with_protocol(protocol)
        .wrap(TransactionMiddleware::new(
            pool.clone(),
            Arc::clone(&session),
            peer,
        ))?
        .app_data(RouteState::new(pool, argon2, handshake));

    let handler = routing_placeholder_handler();
    let app = app.route(FALLBACK_ROUTE_ID, handler.clone())?;
    ROUTE_IDS
        .iter()
        .try_fold(app, |app, id| app.route(*id, handler.clone()))
}

fn routing_placeholder_handler() -> Handler<Envelope> {
    // Wireframe requires a handler per route; transaction middleware owns replies.
    Arc::new(|_: &Envelope| Box::pin(async {}))
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
