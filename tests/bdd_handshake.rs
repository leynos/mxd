use std::cell::RefCell;

use mxd::protocol;
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};

#[derive(Default)]
struct HandshakeWorld {
    buffer: [u8; protocol::HANDSHAKE_LEN],
    parsed: Option<Result<protocol::Handshake, protocol::HandshakeError>>,
}

#[fixture]
fn handshake_world() -> RefCell<HandshakeWorld> {
    RefCell::new(HandshakeWorld::default())
}

#[allow(clippy::needless_pass_by_value)]
#[given("a handshake buffer with protocol {magic} and version {version:u16}")]
fn configure_handshake(handshake_world: &RefCell<HandshakeWorld>, magic: String, version: u16) {
    assert!(magic.len() == 4, "protocol magic must contain four characters");
    let mut state = handshake_world.borrow_mut();
    state.buffer = [0u8; protocol::HANDSHAKE_LEN];
    state.buffer[0..4].copy_from_slice(magic.as_bytes());
    state.buffer[4..8].copy_from_slice(&0u32.to_be_bytes());
    state.buffer[8..10].copy_from_slice(&version.to_be_bytes());
    state.buffer[10..12].copy_from_slice(&0u16.to_be_bytes());
}

#[when("the handshake is parsed")]
fn parse_handshake(handshake_world: &RefCell<HandshakeWorld>) {
    let mut state = handshake_world.borrow_mut();
    state.parsed = Some(protocol::parse_handshake(&state.buffer));
}

#[then("the handshake result is accepted")]
fn assert_success(handshake_world: &RefCell<HandshakeWorld>) {
    let state = handshake_world.borrow();
    let result = state.parsed.as_ref().expect("handshake parsed");
    assert!(result.is_ok(), "expected handshake ok, got {result:?}");
}

#[then("the handshake result is rejected with code {code:u32}")]
fn assert_error(handshake_world: &RefCell<HandshakeWorld>, code: u32) {
    let state = handshake_world.borrow();
    let result = state.parsed.as_ref().expect("handshake parsed");
    let err = result.as_ref().expect_err("expected handshake error");
    let actual = protocol::handshake_error_code(err);
    assert_eq!(actual, code, "unexpected handshake code");
}

#[scenario(path = "tests/features/handshake.feature", index = 0)]
fn handshake_accepts(handshake_world: RefCell<HandshakeWorld>) { let _ = handshake_world; }

#[scenario(path = "tests/features/handshake.feature", index = 1)]
fn handshake_rejects(handshake_world: RefCell<HandshakeWorld>) { let _ = handshake_world; }
