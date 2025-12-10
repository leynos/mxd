//! Hotline handshake preamble for the Wireframe transport.
//!
//! The `wireframe` server reads a fixed-length preamble before routing any
//! messages. Hotline expects a 12-byte header containing a protocol ID,
//! sub-protocol, version, and sub-version. This module validates that payload
//! and exposes a `Preamble` implementation so the Wireframe runtime can reject
//! malformed handshakes before invoking domain logic.

use bincode::{
    de::{BorrowDecode, BorrowDecoder, read::Reader},
    error::DecodeError,
};

use crate::protocol::{
    HANDSHAKE_INVALID_PROTOCOL_TOKEN,
    HANDSHAKE_LEN,
    HANDSHAKE_UNSUPPORTED_VERSION_TOKEN,
    Handshake,
    HandshakeError,
    parse_handshake,
};

/// Validated Hotline preamble decoded by the Wireframe server.
///
/// The preamble embeds the parsed [`Handshake`] so subsequent middleware can
/// branch on the sub-protocol or sub-version without reparsing the raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotlinePreamble {
    handshake: Handshake,
}

impl HotlinePreamble {
    /// Return the parsed handshake payload.
    #[must_use]
    pub const fn handshake(&self) -> &Handshake { &self.handshake }
}

impl From<HotlinePreamble> for Handshake {
    fn from(value: HotlinePreamble) -> Self { value.handshake }
}

impl TryFrom<[u8; HANDSHAKE_LEN]> for HotlinePreamble {
    type Error = HandshakeError;

    fn try_from(bytes: [u8; HANDSHAKE_LEN]) -> Result<Self, Self::Error> {
        parse_handshake(&bytes).map(|handshake| Self { handshake })
    }
}

impl<'de> BorrowDecode<'de, ()> for HotlinePreamble {
    fn borrow_decode<D: BorrowDecoder<'de, Context = ()>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        let mut buf = [0u8; HANDSHAKE_LEN];
        decoder.reader().read(&mut buf)?;
        parse_handshake(&buf)
            .map(|handshake| Self { handshake })
            .map_err(|err| decode_error_for_handshake(&err))
    }
}

fn decode_error_for_handshake(err: &HandshakeError) -> DecodeError {
    match err {
        HandshakeError::InvalidProtocol => DecodeError::OtherString(format!(
            "{HANDSHAKE_INVALID_PROTOCOL_TOKEN}: invalid protocol id"
        )),
        HandshakeError::UnsupportedVersion(ver) => DecodeError::OtherString(format!(
            "{HANDSHAKE_UNSUPPORTED_VERSION_TOKEN}: unsupported version {ver}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use rstest::rstest;
    use tokio::io::BufReader;
    use wireframe::preamble::read_preamble;

    use super::*;
    use crate::{
        protocol::{PROTOCOL_ID, VERSION},
        wireframe::test_helpers::preamble_bytes,
    };

    #[rstest]
    #[tokio::test]
    async fn decodes_valid_preamble() {
        let bytes = preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 7);
        let mut reader = BufReader::new(Cursor::new(bytes));

        let (preamble, leftover) = read_preamble::<_, HotlinePreamble>(&mut reader)
            .await
            .expect("handshake must decode");

        assert!(leftover.is_empty());
        assert_eq!(
            preamble.handshake.sub_protocol,
            u32::from_be_bytes(*b"CHAT")
        );
        assert_eq!(preamble.handshake.version, VERSION);
        assert_eq!(preamble.handshake.sub_version, 7);
    }

    #[rstest]
    #[tokio::test]
    async fn rejects_invalid_protocol() {
        let bytes = preamble_bytes(*b"WRNG", *b"CHAT", VERSION, 1);
        let mut reader = BufReader::new(Cursor::new(bytes));

        let err = read_preamble::<_, HotlinePreamble>(&mut reader)
            .await
            .expect_err("decode must fail");

        assert!(
            err.to_string().contains("invalid protocol id"),
            "expected detailed message, got {err:?}"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn rejects_unsupported_version() {
        let bytes = preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION + 1, 0);
        let mut reader = BufReader::new(Cursor::new(bytes));

        let err = read_preamble::<_, HotlinePreamble>(&mut reader)
            .await
            .expect_err("decode must fail");

        assert!(
            err.to_string().contains("unsupported version"),
            "expected detailed message, got {err:?}"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn propagates_truncated_input() {
        let bytes = &preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 0)[..6];
        let mut reader = BufReader::new(Cursor::new(bytes));

        let err = read_preamble::<_, HotlinePreamble>(&mut reader)
            .await
            .expect_err("decode must fail");

        assert!(matches!(
            err,
            DecodeError::UnexpectedEnd { additional: _ } | DecodeError::Io { additional: _, .. }
        ));
    }
}

#[cfg(test)]
mod bdd {
    use std::cell::RefCell;

    use bincode::{borrow_decode_from_slice, config, error::DecodeError};
    use rstest::fixture;
    use rstest_bdd::{assert_step_err, assert_step_ok};
    use rstest_bdd_macros::{given, scenario, then, when};

    use super::*;
    use crate::{
        protocol::{PROTOCOL_ID, VERSION},
        wireframe::test_helpers::preamble_bytes,
    };

    #[derive(Default)]
    struct HandshakeWorld {
        bytes: RefCell<Vec<u8>>,
        outcome: RefCell<Option<Result<HotlinePreamble, DecodeError>>>,
    }

    impl HandshakeWorld {
        fn set_bytes(&self, bytes: &[u8]) {
            let mut target = self.bytes.borrow_mut();
            target.clear();
            target.extend_from_slice(bytes);
        }

        fn decode(&self) {
            let cfg = config::standard()
                .with_big_endian()
                .with_fixed_int_encoding();
            let result = borrow_decode_from_slice::<HotlinePreamble, _>(&self.bytes.borrow(), cfg)
                .map(|(preamble, _)| preamble);
            self.outcome.borrow_mut().replace(result);
        }
    }

    #[fixture]
    fn world() -> HandshakeWorld {
        let world = HandshakeWorld::default();
        world.set_bytes(&preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 7));
        world
    }

    fn preamble_for_kind(kind: &str) -> [u8; HANDSHAKE_LEN] {
        match kind {
            "wrong-protocol" => preamble_bytes(*b"WRNG", *b"CHAT", VERSION, 1),
            "unsupported-ver" => preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION + 1, 0),
            "truncated" => preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 0),
            other => panic!("unknown malformed preamble kind '{other}'"),
        }
    }

    #[given("a valid wireframe handshake preamble")]
    fn given_valid(world: &HandshakeWorld) {
        world.set_bytes(&preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 7));
    }

    #[given("a malformed wireframe preamble with kind \"{kind}\"")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "rstest-bdd step parameters must be owned; keep String until macro supports &str \
                  captures"
    )]
    fn given_malformed(world: &HandshakeWorld, kind: String) {
        let bytes = preamble_for_kind(&kind);
        if kind == "truncated" {
            world.set_bytes(&bytes[..6]);
        } else {
            world.set_bytes(&bytes);
        }
    }

    #[when("I decode the wireframe preamble")]
    fn when_decode(world: &HandshakeWorld) { world.decode(); }

    #[then("the wireframe preamble decodes successfully")]
    fn then_success(world: &HandshakeWorld) {
        let outcome_ref = world.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("decode not executed");
        };
        assert_step_ok!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "rstest-bdd step parameters must be owned; keep String until macro supports &str \
                  captures"
    )]
    #[then("the sub-protocol is \"{tag}\"")]
    fn then_sub_protocol(world: &HandshakeWorld, tag: String) {
        let outcome_ref = world.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("decode not executed");
        };
        let preamble = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
        let bytes = tag.as_bytes();
        assert_eq!(bytes.len(), 4, "sub-protocol tags must be four bytes");
        let mut buf = [0u8; 4];
        buf.copy_from_slice(bytes);
        assert_eq!(preamble.handshake.sub_protocol, u32::from_be_bytes(buf));
    }

    #[then("the handshake version is {version}")]
    fn then_version(world: &HandshakeWorld, version: u16) {
        let outcome_ref = world.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("decode not executed");
        };
        let preamble = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
        assert_eq!(preamble.handshake.version, version);
    }

    #[then("the handshake sub-version is {sub_version}")]
    fn then_sub_version(world: &HandshakeWorld, sub_version: u16) {
        let outcome_ref = world.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("decode not executed");
        };
        let preamble = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
        assert_eq!(preamble.handshake.sub_version, sub_version);
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "rstest-bdd step parameters must be owned; keep String until macro supports &str \
                  captures"
    )]
    #[then("decoding fails with \"{message}\"")]
    fn then_failure(world: &HandshakeWorld, message: String) {
        let outcome_ref = world.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("decode not executed");
        };
        let text = assert_step_err!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
        assert!(
            text.contains(&message),
            "expected '{text}' to contain '{message}'"
        );
    }

    #[scenario(path = "tests/features/wireframe_handshake.feature", index = 0)]
    fn accepts_preamble(world: HandshakeWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_handshake.feature", index = 1)]
    fn rejects_wrong_protocol(world: HandshakeWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_handshake.feature", index = 2)]
    fn rejects_unsupported_version(world: HandshakeWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_handshake.feature", index = 3)]
    fn rejects_truncated(world: HandshakeWorld) { let _ = world; }
}
