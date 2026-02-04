//! Unit tests for the wireframe server bootstrap.

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
    assert_eq!(
        bootstrap.bind_addr,
        "127.0.0.1:7777".parse().expect("valid socket address")
    );
    assert_eq!(bootstrap.config.bind, "127.0.0.1:7777");
}
