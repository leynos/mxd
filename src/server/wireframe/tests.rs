//! Unit tests for the wireframe server bootstrap.

use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};

use argon2::Argon2;
use rstest::{fixture, rstest};
#[cfg(feature = "sqlite")]
use tempfile::TempDir;
use tokio::runtime::Builder;
use wireframe::WireframeError;

use super::*;
use crate::{
    presence::PresenceRegistry,
    wireframe::{
        connection::{
            ConnectionContext,
            HandshakeMetadata,
            scope_current_context,
            store_current_context,
            take_current_context,
        },
        outbound::WireframeOutboundRegistry,
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
    assert_eq!(
        bootstrap.bind_addr,
        "127.0.0.1:7777".parse().expect("valid socket address")
    );
    assert_eq!(bootstrap.config.bind, "127.0.0.1:7777");
}

fn run_factory_with_stored_context(
    stored: Option<ConnectionContext>,
) -> std::result::Result<HotlineApp, AppFactoryError> {
    run_factory_with_stored_context_and_assertions(stored, || {})
}
fn run_factory_with_stored_context_and_assertions<F>(
    stored: Option<ConnectionContext>,
    assertions: F,
) -> std::result::Result<HotlineApp, AppFactoryError>
where
    F: FnOnce(),
{
    let pool = dummy_pool();
    let argon2 = Arc::new(Argon2::default());
    let outbound_registry = Arc::new(WireframeOutboundRegistry::default());
    let presence = Arc::new(PresenceRegistry::default());
    let runtime = match Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => return Err(AppFactoryError::BuildApplication(err.into())),
    };

    runtime.block_on(scope_current_context(None, async {
        match stored {
            Some(ctx) => store_current_context(ctx),
            None => {
                let _ = take_current_context();
            }
        }
        let app = build_app_for_connection(&pool, &argon2, &outbound_registry, &presence);
        assertions();
        app
    }))
}

#[rstest]
fn app_factory_rejects_missing_handshake_context() {
    let Err(AppFactoryError::MissingHandshakeContext) = run_factory_with_stored_context(None)
    else {
        panic!("missing context must fail closed with the proper error variant");
    };
}

#[rstest]
fn app_factory_builds_when_handshake_context_is_present() {
    let peer = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 5500));
    let context = ConnectionContext::new(HandshakeMetadata::default()).with_peer(peer);

    let app = run_factory_with_stored_context_and_assertions(Some(context), || {
        assert!(
            take_current_context().is_none(),
            "per-connection context must be consumed by the app factory"
        );
    });

    let app = app.expect("app factory should succeed when handshake context is present");
    assert!(app.message_assembler().is_some());
}

#[rstest]
fn app_factory_rejects_missing_peer_address() {
    let context = ConnectionContext::new(HandshakeMetadata::default());
    let Err(AppFactoryError::MissingPeerAddress) = run_factory_with_stored_context(Some(context))
    else {
        panic!("missing peer address must fail closed with the proper error variant");
    };
}

/// A Wireframe start-up audit failure must beat the listener bind.
///
/// The audit sits between `establish_pool` and `WireframeServer::bind`, and
/// that ordering is the whole point of the check: an incompatible library must
/// fail startup, not the first query. Asserting only that `run` returns the
/// audit error cannot detect a misordering, because a run that bound first and
/// audited second would still report the audit failure.
///
/// The address is therefore bound by this test and held for the duration. With
/// an occupied address the outcomes separate cleanly: an audit before the bind
/// returns the audit error, while a bind before the audit fails with
/// `failed to bind wireframe server`. A free address would be worse than
/// useless here, because a run that dropped the audit would bind and serve
/// until the test timed out instead of failing.
///
/// The audit seam is thread-local, so the runtime flavour is load-bearing:
/// under a multi-threaded runtime `run` could resume on a different polling
/// thread after the pool is established and lose the guard. The flavour is
/// spelled out rather than left to the macro default, because a future tokio
/// change to that default would silently make this test pass for the wrong
/// reason. On a current-thread runtime the whole of `run` is polled on this
/// thread, including the awaits inside the audit.
///
/// Migration ordering is deliberately not asserted: this path establishes a
/// pool and runs no migrations at all.
#[cfg(feature = "sqlite")]
#[tokio::test(flavor = "current_thread")]
async fn run_audits_before_binding_the_listener() {
    let occupied = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("occupying listener must bind");
    let bind_addr = occupied
        .local_addr()
        .expect("occupied address must resolve");
    let temp_dir = TempDir::new().expect("temporary directory must be created");
    let config = AppConfig {
        bind: bind_addr.to_string(),
        database: temp_dir
            .path()
            .join("audited.mxd.db")
            .to_string_lossy()
            .into_owned(),
        // Cheap Argon2 parameters: the audit fails first, but a misordered run
        // would reach key derivation and the default cost is needlessly slow.
        argon2_m_cost: 1024,
        argon2_t_cost: 1,
        argon2_p_cost: 1,
        ..AppConfig::default()
    };
    let bootstrap = WireframeBootstrap::prepare(config).expect("bootstrap must prepare");

    let error = {
        // Held across the whole `run` call, including its awaits.
        let _injection = crate::db::fail_next_audit();
        bootstrap
            .run()
            .await
            .expect_err("an audit failure must abort startup")
    };

    let chain = format!("{error:#}");
    assert!(
        chain.contains("SQLite capability audit failed"),
        "the error must carry the audit context: {chain}"
    );
    assert!(
        !chain.contains("failed to bind wireframe server"),
        "the audit must run before the listener binds: {chain}"
    );
}

#[rstest]
fn app_factory_wraps_build_application_errors() {
    let duplicate_route = FALLBACK_ROUTE_ID;
    let result = map_build_application_result(Err(WireframeError::DuplicateRoute(duplicate_route)));

    let Err(AppFactoryError::BuildApplication(error)) = result else {
        panic!("wireframe builder errors must be wrapped in AppFactoryError::BuildApplication");
    };

    let message = error.to_string();
    assert!(
        message.contains("wireframe error:"),
        "wrapped error should include the wireframe prefix"
    );
    assert!(
        message.contains(&format!(
            "route id {duplicate_route} was already registered"
        )),
        "wrapped error should preserve the wireframe failure details"
    );
}
