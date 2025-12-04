//! Wireframe-based server runtime.
//!
//! This module bootstraps the upcoming Wireframe transport adapter while
//! reusing the shared CLI and configuration plumbing housed in the library.
//! The initial implementation keeps the runtime intentionally small: it
//! parses configuration, resolves the bind address, and starts an empty
//! [`WireframeServer`]. Future work will register the Hotline handshake,
//! serializer, and protocol routes described in the roadmap.

#[cfg(test)]
use std::sync::{Mutex, OnceLock};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use tracing::warn;
use wireframe::{
    app::WireframeApp,
    server::{BackoffConfig, WireframeServer},
};

use super::{AppConfig, Cli};
use crate::{
    protocol,
    server::admin,
    wireframe::{
        connection::{HandshakeMetadata, clear_current_handshake, current_handshake},
        handshake,
        preamble::HotlinePreamble,
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
/// app data alongside the shared configuration, and then clears the metadata
/// to prevent leakage into subsequent connections. Call this exactly once per
/// accepted connection after the handshake completes.
fn build_app(config: Arc<AppConfig>) -> WireframeApp {
    let handshake = current_handshake().unwrap_or_else(|| {
        warn!("handshake metadata missing; defaulting to zeroed values");
        HandshakeMetadata::default()
    });
    #[cfg(test)]
    {
        let last = LAST_HANDSHAKE.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = last.lock() {
            guard.replace(handshake.clone());
        }
    }
    let app = WireframeApp::default().app_data(config).app_data(handshake);
    clear_current_handshake();
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
        let WireframeBootstrap {
            bind_addr,
            config,
            backoff,
        } = self;
        println!("mxd-wireframe-server using database {}", config.database);
        println!("mxd-wireframe-server binding to {}", config.bind);
        let config_for_app = Arc::clone(&config);
        let server = WireframeServer::new(move || build_app(Arc::clone(&config_for_app)))
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

    use super::*;
    use crate::{
        protocol::VERSION,
        wireframe::connection::{clear_current_handshake, store_current_handshake},
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
    fn build_app_uses_current_handshake(bound_config: AppConfig) {
        let meta = HandshakeMetadata {
            sub_protocol: u32::from_be_bytes(*b"CHAT"),
            version: VERSION,
            sub_version: 7,
        };
        store_current_handshake(meta.clone());

        let _app = build_app(Arc::new(bound_config));

        assert!(current_handshake().is_none(), "handshake should be cleared");
        let recorded = take_last_handshake().expect("handshake recorded");
        assert_eq!(recorded, meta);
    }

    #[rstest]
    fn build_app_defaults_when_missing(bound_config: AppConfig) {
        clear_current_handshake();

        let _app = build_app(Arc::new(bound_config));

        assert!(current_handshake().is_none(), "handshake should be cleared");
        let recorded = take_last_handshake().expect("handshake recorded");
        assert_eq!(recorded, HandshakeMetadata::default());
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
