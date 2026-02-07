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
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            base: WireframeBddWorld::new()?,
        })
    }

    const fn is_skipped(&self) -> bool { self.base.is_skipped() }

    fn setup_db(&self, setup: SetupFn) -> Result<(), Box<dyn std::error::Error>> {
        self.base.setup_db(setup).map_err(Into::into)
    }

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

    fn assertion_error(message: impl Into<String>) -> Box<dyn std::error::Error> {
        std::io::Error::other(message.into()).into()
    }

    fn assert_includes_banner_fields(
        params: &[(FieldId, Vec<u8>)],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let banner = params
            .iter()
            .find(|(id, _)| *id == FieldId::BannerId)
            .ok_or_else(|| Self::assertion_error("missing banner id"))?;
        let server = params
            .iter()
            .find(|(id, _)| *id == FieldId::ServerName)
            .ok_or_else(|| Self::assertion_error("missing server name"))?;

        if banner.1 != [0u8, 0u8, 0u8, 0u8] {
            return Err(Self::assertion_error(format!(
                "expected banner id bytes [0, 0, 0, 0], got {:?}",
                banner.1
            )));
        }
        if server.1 != b"mxd" {
            return Err(Self::assertion_error(format!(
                "expected server name bytes b\"mxd\", got {:?}",
                server.1
            )));
        }
        Ok(())
    }

    fn assert_omits_banner_fields(
        params: &[(FieldId, Vec<u8>)],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if params.iter().any(|(id, _)| *id == FieldId::BannerId) {
            return Err(Self::assertion_error(
                "expected login reply without BannerId field",
            ));
        }
        if params.iter().any(|(id, _)| *id == FieldId::ServerName) {
            return Err(Self::assertion_error(
                "expected login reply without ServerName field",
            ));
        }
        Ok(())
    }

    fn assert_banner_fields(&self, should_include: bool) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_skipped() {
            return Ok(());
        }
        self.with_reply(|tx| {
            if tx.header.error != 0 {
                return Err(Self::assertion_error(format!(
                    "expected successful reply (error = 0), got {}",
                    tx.header.error
                )));
            }
            let tx_type = TransactionType::from(tx.header.ty);
            if tx_type != TransactionType::Login {
                return Err(Self::assertion_error(format!(
                    "expected Login reply, got {tx_type:?}"
                )));
            }
            let params = decode_params(&tx.payload)?;
            if should_include {
                Self::assert_includes_banner_fields(&params)?;
            } else {
                Self::assert_omits_banner_fields(&params)?;
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
        Ok(())
    }
}

#[fixture]
fn world() -> LoginCompatWorld {
    let world = match LoginCompatWorld::new() {
        Ok(world) => world,
        Err(error) => {
            panic!("failed to construct login compatibility fixture world runtime: {error}")
        }
    };
    assert!(!world.is_skipped());
    world
}

#[given("a routing context with user accounts")]
fn given_users(world: &LoginCompatWorld) -> Result<(), Box<dyn std::error::Error>> {
    world.setup_db(setup_login_db)
}

#[given("a handshake sub-version {sub_version}")]
fn given_sub_version(world: &LoginCompatWorld, sub_version: u16) {
    world.set_handshake_sub_version(sub_version);
}

#[when("I send a login request with client version {version}")]
fn when_login(world: &LoginCompatWorld, version: u16) { world.send_login(version); }

#[then("the login reply includes banner fields")]
fn then_includes_banner_fields(world: &LoginCompatWorld) -> Result<(), Box<dyn std::error::Error>> {
    world.assert_banner_fields(true)
}

#[then("the login reply omits banner fields")]
fn then_omits_banner_fields(world: &LoginCompatWorld) -> Result<(), Box<dyn std::error::Error>> {
    world.assert_banner_fields(false)
}

scenarios!(
    "tests/features/wireframe_login_compat.feature",
    fixtures = [world: LoginCompatWorld]
);
