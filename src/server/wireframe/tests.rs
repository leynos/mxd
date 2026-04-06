//! Unit tests for the wireframe server bootstrap.

use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};

use argon2::Argon2;
use rstest::{fixture, rstest};

use super::*;
use crate::wireframe::{
    connection::{
        ConnectionContext,
        HandshakeMetadata,
        store_current_context,
        take_current_context,
    },
    test_helpers::dummy_pool,
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

#[rstest]
fn app_factory_rejects_missing_handshake_context() {
    let pool = dummy_pool();
    let argon2 = Arc::new(Argon2::default());
    let outbound_registry = Arc::new(WireframeOutboundRegistry::default());

    let Err(err) = build_app_for_connection(&pool, &argon2, &outbound_registry) else {
        panic!("missing context must fail closed");
    };

    assert!(err.to_string().contains("missing handshake context"));
}

#[rstest]
fn app_factory_builds_when_handshake_context_is_present() {
    let pool = dummy_pool();
    let argon2 = Arc::new(Argon2::default());
    let outbound_registry = Arc::new(WireframeOutboundRegistry::default());
    let peer = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 5500));
    let context = ConnectionContext::new(HandshakeMetadata::default()).with_peer(peer);
    store_current_context(context);

    let app = build_app_for_connection(&pool, &argon2, &outbound_registry);
    let _ = take_current_context();

    assert!(app.is_ok());
}
