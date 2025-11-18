//! Wireframe-based server runtime.
//!
//! This module bootstraps the upcoming Wireframe transport adapter while
//! reusing the shared CLI and configuration plumbing housed in the library.
//! The initial implementation keeps the runtime intentionally small: it
//! parses configuration, resolves the bind address, and starts an empty
//! [`WireframeServer`]. Future work will register the Hotline handshake,
//! serializer, and protocol routes described in the roadmap.

use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use clap::Parser;
use wireframe::{
    app::WireframeApp,
    server::{BackoffConfig, WireframeServer},
};

use super::{AppConfig, Cli};
use crate::server::legacy;

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
        legacy::run_command(command, &config).await
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
    state: WireframeState,
    backoff: BackoffConfig,
}

impl WireframeBootstrap {
    fn prepare(config: AppConfig) -> Result<Self> {
        let bind_addr = parse_bind_addr(&config.bind)?;
        Ok(Self {
            bind_addr,
            state: WireframeState::new(config),
            backoff: BackoffConfig::default(),
        })
    }

    async fn run(self) -> Result<()> {
        let WireframeBootstrap {
            bind_addr,
            state,
            backoff,
        } = self;
        println!("mxd-wireframe-server using database {}", state.database());
        println!("mxd-wireframe-server binding to {}", state.bind());
        let server = WireframeServer::new(app_factory(state)).accept_backoff(backoff);
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

#[derive(Clone, Debug)]
struct WireframeState {
    config: Arc<AppConfig>,
}

impl WireframeState {
    fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    fn bind(&self) -> &str { &self.config.bind }

    fn database(&self) -> &str { &self.config.database }
}

fn app_factory(state: WireframeState) -> impl Fn() -> WireframeApp + Send + Sync + Clone + 'static {
    move || WireframeApp::default().app_data(state.clone())
}

fn parse_bind_addr(target: &str) -> Result<SocketAddr> {
    target
        .parse()
        .with_context(|| format!("invalid bind address '{target}'"))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn config_with_bind(bind: &str) -> AppConfig {
        AppConfig {
            bind: bind.to_string(),
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
    fn bootstrap_captures_bind() {
        let cfg = config_with_bind("127.0.0.1:7777");
        let bootstrap = WireframeBootstrap::prepare(cfg).expect("bootstrap");
        assert_eq!(bootstrap.bind_addr, "127.0.0.1:7777".parse().unwrap());
        assert_eq!(bootstrap.state.bind(), "127.0.0.1:7777");
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
        assert!(
            world.config.borrow().bind.contains(':'),
            "fixture must expose a host:port binding"
        );
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
