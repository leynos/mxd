//! Behavioural tests for login compatibility gating.

use mxd::{
    field_id::FieldId,
    transaction::{Transaction, decode_params},
    transaction_type::TransactionType,
    wireframe::connection::HandshakeMetadata,
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenarios, then, when};
use test_util::{SetupFn, WireframeBddWorld, build_frame, setup_login_db};

/// Shared BDD world state for login compatibility scenarios.
struct LoginCompatWorld {
    base: WireframeBddWorld,
}

impl LoginCompatWorld {
    fn new() -> Self {
        Self {
            base: WireframeBddWorld::new(),
        }
    }

    const fn is_skipped(&self) -> bool { self.base.is_skipped() }

    fn setup_db(&self, setup: SetupFn) { self.base.setup_db(setup); }

    fn set_handshake_sub_version(&self, sub_version: u16) {
        if self.is_skipped() {
            return;
        }
        let handshake = HandshakeMetadata {
            sub_version,
            ..HandshakeMetadata::default()
        };
        self.base.set_client_compat_from_handshake(&handshake);
    }

    fn send_login(&self, version: u16) {
        if self.is_skipped() {
            return;
        }
        #[expect(
            clippy::big_endian_bytes,
            reason = "test fixture uses explicit network byte order payload"
        )]
        let version_bytes = version.to_be_bytes();
        let frame = match build_frame(
            TransactionType::Login,
            1,
            &[
                (FieldId::Login, b"alice"),
                (FieldId::Password, b"secret"),
                (FieldId::Version, version_bytes.as_slice()),
            ],
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.base.set_reply_error(err.to_string());
                return;
            }
        };
        self.base.send_raw(&frame);
    }

    fn with_reply<T>(&self, f: impl FnOnce(&Transaction) -> T) -> T { self.base.with_reply(f) }

    fn assert_banner_fields(&self, should_include: bool) {
        if self.is_skipped() {
            return;
        }
        self.with_reply(|tx| {
            assert_eq!(
                tx.header.error, 0,
                "expected successful reply (error = 0), got {}",
                tx.header.error
            );
            let tx_type = TransactionType::from(tx.header.ty);
            assert_eq!(
                tx_type,
                TransactionType::Login,
                "expected Login reply, got {tx_type:?}"
            );
            let params = decode_params(&tx.payload)
                .unwrap_or_else(|err| panic!("failed to decode reply params: {err}"));
            let banner_field = params.iter().find(|(id, _)| *id == FieldId::BannerId);
            let server_field = params.iter().find(|(id, _)| *id == FieldId::ServerName);

            if should_include {
                let (banner, server) = match (banner_field, server_field) {
                    (Some(banner), Some(server)) => (banner, server),
                    (None, _) => panic!("missing banner id"),
                    (_, None) => panic!("missing server name"),
                };
                assert_eq!(banner.1, [0u8, 0u8, 0u8, 0u8]);
                assert_eq!(server.1, b"mxd");
            } else {
                assert!(
                    banner_field.is_none(),
                    "expected no banner_field for this client when should_include is false"
                );
                assert!(
                    server_field.is_none(),
                    "expected no server_field for this client when should_include is false"
                );
            }
        });
    }
}

#[fixture]
fn world() -> LoginCompatWorld {
    let world = LoginCompatWorld::new();
    assert!(!world.is_skipped());
    world
}

#[given("a routing context with user accounts")]
fn given_users(world: &LoginCompatWorld) { world.setup_db(setup_login_db); }

#[given("a handshake sub-version {sub_version}")]
fn given_sub_version(world: &LoginCompatWorld, sub_version: u16) {
    world.set_handshake_sub_version(sub_version);
}

#[when("I send a login request with client version {version}")]
fn when_login(world: &LoginCompatWorld, version: u16) { world.send_login(version); }

#[then("the login reply includes banner fields")]
fn then_includes_banner_fields(world: &LoginCompatWorld) { world.assert_banner_fields(true); }

#[then("the login reply omits banner fields")]
fn then_omits_banner_fields(world: &LoginCompatWorld) { world.assert_banner_fields(false); }

scenarios!(
    "tests/features/wireframe_login_compat.feature",
    fixtures = [world: LoginCompatWorld]
);
