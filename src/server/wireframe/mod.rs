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
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use argon2::Argon2;
use tokio::sync::Mutex as TokioMutex;
use tracing::{error, warn};
use wireframe::{
    app::{Envelope, Handler, WireframeApp},
    serializer::BincodeSerializer,
    server::WireframeServer,
};

use super::{AppConfig, ResolvedCli, load_cli};
use crate::{
    db::{DbPool, establish_pool},
    handler::Session,
    protocol,
    server::admin,
    wireframe::{
        codec::HotlineFrameCodec,
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        connection::{HandshakeMetadata, take_current_context},
        handshake,
        outbound::{
            WireframeOutboundConnection,
            WireframeOutboundMessaging,
            WireframeOutboundRegistry,
        },
        preamble::HotlinePreamble,
        protocol::HotlineProtocol,
        route_ids::{FALLBACK_ROUTE_ID, ROUTE_IDS},
        routes::{TransactionMiddleware, TransactionMiddlewareConfig},
    },
};

type HotlineApp = WireframeApp<BincodeSerializer, (), Envelope, HotlineFrameCodec>;

/// Parse CLI arguments and start the Wireframe runtime.
///
/// # Errors
///
/// Returns any error emitted while parsing configuration or starting the Wireframe runtime.
pub async fn run() -> Result<()> {
    let cli = load_cli()?;
    run_with_cli(cli).await
}

/// Execute the Wireframe runtime with a resolved [`ResolvedCli`].
///
/// # Errors
///
/// Returns any error raised while running administrative commands or binding
/// the Wireframe listener.
pub async fn run_with_cli(cli: ResolvedCli) -> Result<()> {
    let ResolvedCli { config, command } = cli;
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

        let outbound_registry = Arc::new(WireframeOutboundRegistry::default());
        validate_app_factory(&pool, &argon2, &outbound_registry)
            .context("failed to validate wireframe app factory")?;
        let app_factory = {
            let pool = pool.clone();
            let argon2 = Arc::clone(&argon2);
            let outbound_registry = Arc::clone(&outbound_registry);
            move || build_app_for_connection(&pool, &argon2, &outbound_registry)
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

fn build_app_for_connection(
    pool: &DbPool,
    argon2: &Arc<Argon2<'static>>,
    outbound_registry: &Arc<WireframeOutboundRegistry>,
) -> HotlineApp {
    build_app_with_logging(pool, argon2, outbound_registry)
}

fn build_app_with_logging(
    pool: &DbPool,
    argon2: &Arc<Argon2<'static>>,
    outbound_registry: &Arc<WireframeOutboundRegistry>,
) -> HotlineApp {
    match try_build_app(pool, argon2, outbound_registry) {
        Ok(app) => app,
        Err(err) => {
            error!(error = %err, "failed to build wireframe application");
            fallback_app()
        }
    }
}

fn try_build_app(
    pool: &DbPool,
    argon2: &Arc<Argon2<'static>>,
    outbound_registry: &Arc<WireframeOutboundRegistry>,
) -> Result<HotlineApp> {
    let build_context = build_app_context(pool, argon2, outbound_registry)
        .context("failed to build wireframe app context")?;
    build_app(build_context).context("failed to build wireframe application")
}

fn build_app_context<'a>(
    pool: &'a DbPool,
    argon2: &'a Arc<Argon2<'static>>,
    outbound_registry: &'a Arc<WireframeOutboundRegistry>,
) -> Result<AppBuildContext<'a>> {
    // Missing connection context indicates handshake setup failed; abort the
    // connection rather than running without routing state. Returning a
    // degraded app would accept traffic with broken routing and state.
    let context = take_current_context()
        .ok_or_else(|| anyhow!("missing handshake context in app factory"))?;
    let (handshake, peer) = context.into_parts();
    let peer = peer.ok_or_else(|| anyhow!("peer address missing in app factory"))?;
    let compat = Arc::new(XorCompatibility::from_handshake(&handshake));
    let client_compat = Arc::new(ClientCompatibility::from_handshake(&handshake));
    Ok(AppBuildContext {
        pool,
        argon2,
        outbound_registry,
        peer,
        compat,
        client_compat,
    })
}

struct AppBuildContext<'a> {
    pool: &'a DbPool,
    argon2: &'a Arc<Argon2<'static>>,
    outbound_registry: &'a Arc<WireframeOutboundRegistry>,
    peer: SocketAddr,
    compat: Arc<XorCompatibility>,
    client_compat: Arc<ClientCompatibility>,
}

fn build_app(context: AppBuildContext<'_>) -> wireframe::app::Result<HotlineApp> {
    let AppBuildContext {
        pool,
        argon2,
        outbound_registry,
        peer,
        compat,
        client_compat,
    } = context;
    let session = Arc::new(TokioMutex::new(Session::default()));
    let outbound_id = outbound_registry.allocate_id();
    let outbound_connection = Arc::new(WireframeOutboundConnection::new(
        outbound_id,
        Arc::clone(outbound_registry),
    ));
    let outbound_messaging = WireframeOutboundMessaging::new(Arc::clone(&outbound_connection));
    let protocol = HotlineProtocol::new(
        pool.clone(),
        Arc::clone(argon2),
        outbound_connection,
        Arc::clone(&compat),
    );

    let app = HotlineApp::default()
        .fragmentation(None)
        .with_protocol(protocol)
        .wrap(TransactionMiddleware::new(TransactionMiddlewareConfig {
            pool: pool.clone(),
            session: Arc::clone(&session),
            peer,
            messaging: Arc::new(outbound_messaging),
            compat,
            client_compat,
        }))?;

    let handler = routing_placeholder_handler();
    let app = app.route(FALLBACK_ROUTE_ID, handler.clone())?;
    ROUTE_IDS
        .iter()
        .try_fold(app, |app, id| app.route(*id, handler.clone()))
}

fn validate_app_factory(
    pool: &DbPool,
    argon2: &Arc<Argon2<'static>>,
    outbound_registry: &Arc<WireframeOutboundRegistry>,
) -> Result<()> {
    let peer = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));
    let build_context = AppBuildContext {
        pool,
        argon2,
        outbound_registry,
        peer,
        compat: Arc::new(XorCompatibility::disabled()),
        client_compat: Arc::new(ClientCompatibility::from_handshake(
            &HandshakeMetadata::default(),
        )),
    };
    build_app(build_context).context("failed to register routes or middleware")?;
    Ok(())
}

fn fallback_app() -> HotlineApp { HotlineApp::default() }

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
mod tests;

#[cfg(test)]
mod bdd;
