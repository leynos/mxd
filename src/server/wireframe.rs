//! Wireframe-based server runtime.
//!
//! This module bootstraps the Wireframe transport adapter with the
//! [`HotlineProtocol`] registered via `.with_protocol(...)`. The adapter
//! routes incoming Hotline transactions to domain handlers while maintaining
//! per-connection session state.
//!
//! # Architecture
//!
//! The bootstrap process:
//!
//! 1. Establishes a database connection pool
//! 2. Creates a shared Argon2 instance for password hashing
//! 3. Registers the `HotlineProtocol` adapter with the wireframe server
//! 4. Installs the Hotline preamble decoder and handshake hooks
//! 5. Binds to the configured address and starts accepting connections
//!
//! This implementation fulfils the roadmap task "Route transactions through
//! wireframe" by integrating the protocol adapter described in
//! `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`.

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
    net::{Ipv4Addr, SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use argon2::Argon2;
use clap::Parser;
use tokio::sync::Mutex as TokioMutex;
use tracing::warn;
use wireframe::{
    app::{Envelope, WireframeApp},
    server::{BackoffConfig, WireframeServer},
};

use super::{AppConfig, Cli};
use crate::{
    db::{DbPool, establish_pool},
    handler::Session,
    protocol,
    server::admin,
    wireframe::{
        connection::{
            HandshakeMetadata,
            clear_current_handshake,
            clear_current_peer,
            current_handshake,
            current_peer,
        },
        handshake,
        preamble::HotlinePreamble,
        protocol::HotlineProtocol,
        routes::TransactionMiddleware,
    },
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

/// Build a `WireframeApp` for a single connection using the current handshake
/// metadata.
///
/// Reads handshake metadata captured during preamble handling, attaches it to
/// app data alongside the shared configuration, database pool, and Argon2
/// instance. The handshake metadata is cleared after capture to prevent leakage
/// into subsequent connections. Call this exactly once per accepted connection
/// after the handshake completes.
///
/// The `HotlineProtocol` adapter is registered via `.with_protocol()`,
/// providing connection lifecycle hooks for the Hotline protocol.
fn build_app(config: Arc<AppConfig>, pool: DbPool, argon2: Arc<Argon2<'static>>) -> WireframeApp {
    let handshake = current_handshake().unwrap_or_else(|| {
        warn!("handshake metadata missing; defaulting to zeroed values");
        HandshakeMetadata::default()
    });
    let _peer = current_peer().unwrap_or_else(|| {
        warn!("peer address missing; defaulting to unspecified");
        SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)
    });
    #[cfg(test)]
    {
        let last = LAST_HANDSHAKE.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = last.lock() {
            guard.replace(handshake.clone());
        }
    }
    let protocol = HotlineProtocol::new(pool.clone(), Arc::clone(&argon2));
    let session = Arc::new(TokioMutex::new(Session::default()));

    // Register a dummy handler. Middleware intercepts all frames before routing,
    // so this handler is never invoked. The wireframe library requires at least
    // one route to be registered for the middleware infrastructure to work.
    let dummy_handler: wireframe::app::Handler<Envelope> =
        Arc::new(|_env: &Envelope| Box::pin(async {}));

    // Create middleware with pool and session passed directly (not thread-local)
    // to work correctly with Tokio's work-stealing scheduler.
    let middleware = TransactionMiddleware::new(pool.clone(), Arc::clone(&session));

    // These expect() calls are on infallible operations:
    // - WireframeApp::new() only fails on internal errors
    // - route(0, _) only fails for duplicate routes
    // - wrap() middleware registration should not fail
    #[expect(
        clippy::expect_used,
        reason = "wireframe builder operations are infallible for valid inputs"
    )]
    let app = WireframeApp::new()
        .expect("WireframeApp creation should succeed")
        .with_protocol(protocol)
        .route(0, dummy_handler)
        .expect("dummy route registration should succeed")
        .wrap(middleware)
        .expect("middleware registration should succeed")
        .app_data(config)
        .app_data(handshake)
        .app_data(pool)
        .app_data(argon2);
    clear_current_handshake();
    clear_current_peer();
    app
}

#[derive(Clone, Debug)]
struct WireframeBootstrap {
    bind_addr: SocketAddr,
    config: Arc<AppConfig>,
    backoff: BackoffConfig,
}

impl WireframeBootstrap {
    fn prepare(config: AppConfig) -> Result<Self> {
        let bind_addr = parse_bind_addr(&config.bind)?;
        Ok(Self {
            bind_addr,
            config: Arc::new(config),
            backoff: BackoffConfig::default(),
        })
    }

    async fn run(self) -> Result<()> {
        let Self {
            bind_addr,
            config,
            backoff,
        } = self;
        println!("mxd-wireframe-server using database {}", config.database);
        println!("mxd-wireframe-server binding to {}", config.bind);

        // Establish the database connection pool
        let pool = establish_pool(&config.database)
            .await
            .context("failed to establish database pool")?;

        // Create a shared Argon2 instance for password hashing
        let argon2 =
            Arc::new(admin::argon2_from_config(&config).context("failed to configure Argon2")?);

        let config_for_app = Arc::clone(&config);
        let server = WireframeServer::new(move || {
            build_app(
                Arc::clone(&config_for_app),
                pool.clone(),
                Arc::clone(&argon2),
            )
        })
        .with_preamble::<HotlinePreamble>();
        let server =
            handshake::install(server, protocol::HANDSHAKE_TIMEOUT).accept_backoff(backoff);
        let server = server
            .bind(bind_addr)
            .context("failed to bind Wireframe listener")?;
        if let Some(addr) = server.local_addr() {
            println!("mxd-wireframe-server listening on {addr}");
        }
        server
            .run()
            .await
            .context("wireframe server runtime exited with error")
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
    use serial_test::serial;

    use super::*;
    use crate::{
        protocol::VERSION,
        wireframe::{
            connection::{clear_current_handshake, store_current_handshake},
            test_helpers::dummy_pool,
        },
    };

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

    fn take_last_handshake() -> Option<HandshakeMetadata> {
        LAST_HANDSHAKE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    #[rstest]
    #[serial]
    fn build_app_uses_current_handshake(bound_config: AppConfig) {
        let meta = HandshakeMetadata {
            sub_protocol: u32::from_be_bytes(*b"CHAT"),
            version: VERSION,
            sub_version: 7,
        };
        store_current_handshake(meta.clone());

        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let _app = build_app(Arc::new(bound_config), pool, argon2);

        assert!(current_handshake().is_none(), "handshake should be cleared");
        let recorded = take_last_handshake().expect("handshake recorded");
        assert_eq!(recorded, meta);
    }

    #[rstest]
    #[serial]
    fn build_app_defaults_when_missing(bound_config: AppConfig) {
        clear_current_handshake();

        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let _app = build_app(Arc::new(bound_config), pool, argon2);

        assert!(current_handshake().is_none(), "handshake should be cleared");
        let recorded = take_last_handshake().expect("handshake recorded");
        assert_eq!(recorded, HandshakeMetadata::default());
    }

    #[rstest]
    #[serial]
    fn build_app_registers_protocol(bound_config: AppConfig) {
        clear_current_handshake();

        let pool = dummy_pool();
        let argon2 = Arc::new(Argon2::default());
        let app = build_app(Arc::new(bound_config), pool, argon2);

        // Verify the protocol was registered by checking protocol_hooks exist
        let hooks = app.protocol_hooks();
        // The hooks struct should have callbacks registered
        assert!(
            hooks.before_send.is_some() || hooks.on_connection_setup.is_some(),
            "protocol should be registered with hooks"
        );
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
