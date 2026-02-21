//! Behavioural tests for login compatibility gating.

use mxd::{
    commands::ERR_NOT_AUTHENTICATED,
    field_id::FieldId,
    transaction::{Transaction, decode_params},
    transaction_type::TransactionType,
    wireframe::connection::HandshakeMetadata,
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenarios, then, when};
use test_util::{
    SetupFn,
    WireframeBddWorld,
    build_frame,
    ensure_server_binary_env,
    setup_login_db,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClientVersion(u16);

impl ClientVersion {
    const fn new(version: u16) -> Self { Self(version) }

    const fn as_u16(self) -> u16 { self.0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandshakeSubVersion(u16);

impl HandshakeSubVersion {
    const fn new(sub_version: u16) -> Self { Self(sub_version) }

    const fn as_u16(self) -> u16 { self.0 }
}

#[derive(Debug, Clone)]
struct LoginCredentials<'a> {
    username: &'a [u8],
    password: &'a [u8],
}

impl<'a> LoginCredentials<'a> {
    const fn new(username: &'a [u8], password: &'a [u8]) -> Self { Self { username, password } }

    const fn into_parts(self) -> (&'a [u8], &'a [u8]) { (self.username, self.password) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BannerFieldAssertion {
    ShouldInclude,
    ShouldOmit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ErrorCode(i32);

impl ErrorCode {
    const SUCCESS: Self = Self(0);

    const fn new(error: i32) -> Self { Self(error) }

    const fn as_i32(self) -> i32 { self.0 }
}

/// Shared BDD world state for login compatibility scenarios.
struct LoginCompatWorld {
    base: WireframeBddWorld,
}

#[derive(Debug)]
struct AssertionError(String);

impl std::fmt::Display for AssertionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}

impl std::error::Error for AssertionError {}

impl LoginCompatWorld {
    const fn new() -> Self {
        Self {
            base: WireframeBddWorld::new(),
        }
    }

    fn setup_db(&self, setup: SetupFn) -> Result<(), Box<dyn std::error::Error>> {
        self.base.setup_db(setup).map_err(Into::into)
    }

    fn set_handshake_sub_version(&self, handshake_sub_version: HandshakeSubVersion) {
        if self.base.is_skipped() {
            return;
        }
        let handshake = HandshakeMetadata {
            sub_version: handshake_sub_version.as_u16(),
            ..HandshakeMetadata::default()
        };
        self.base.set_client_compat_from_handshake(&handshake);
    }

    fn send_login(&self, version: ClientVersion) {
        self.send_login_with_credentials(version, LoginCredentials::new(b"alice", b"secret"));
    }

    fn send_login_with_credentials(
        &self,
        version: ClientVersion,
        credentials: LoginCredentials<'_>,
    ) {
        if self.base.is_skipped() {
            return;
        }
        #[expect(
            clippy::big_endian_bytes,
            reason = "test fixture uses explicit network byte order payload"
        )]
        let version_bytes = version.as_u16().to_be_bytes();
        let (username, password) = credentials.into_parts();
        let frame = match build_frame(
            TransactionType::Login,
            1,
            &[
                (FieldId::Login, username),
                (FieldId::Password, password),
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
        Box::new(AssertionError(message.into()))
    }

    fn expected_login_error_message(expected_error: i32, actual_error: u32) -> String {
        if expected_error == 0 {
            format!("expected successful reply (error = 0), got {actual_error}")
        } else {
            format!("expected login failure error {expected_error}, got {actual_error}")
        }
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

    fn assert_login_reply(
        &self,
        expected_error: ErrorCode,
        validate_params: impl FnOnce(&[(FieldId, Vec<u8>)]) -> Result<(), Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.base.is_skipped() {
            return Ok(());
        }
        self.with_reply(|tx| {
            let expected_error_value = expected_error.as_i32();
            let actual_error_value = i64::from(tx.header.error);
            if actual_error_value != i64::from(expected_error_value) {
                return Err(Self::assertion_error(Self::expected_login_error_message(
                    expected_error_value,
                    tx.header.error,
                )));
            }
            let tx_type = TransactionType::from(tx.header.ty);
            if tx_type != TransactionType::Login {
                return Err(Self::assertion_error(format!(
                    "expected Login reply, got {tx_type:?}"
                )));
            }
            let params = decode_params(&tx.payload)?;
            validate_params(&params)?;
            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
        Ok(())
    }

    fn assert_banner_fields(
        &self,
        assertion: BannerFieldAssertion,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.assert_login_reply(ErrorCode::SUCCESS, |params| {
            match assertion {
                BannerFieldAssertion::ShouldInclude => Self::assert_includes_banner_fields(params)?,
                BannerFieldAssertion::ShouldOmit => Self::assert_omits_banner_fields(params)?,
            }
            Ok(())
        })
    }

    fn assert_login_fails_without_banner_fields(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.assert_login_reply(
            ErrorCode::new(ERR_NOT_AUTHENTICATED.cast_signed()),
            Self::assert_omits_banner_fields,
        )
    }
}

#[fixture]
fn world() -> LoginCompatWorld {
    ensure_server_binary_env(env!("CARGO_BIN_EXE_mxd-wireframe-server"))
        .unwrap_or_else(|error| panic!("failed to configure wireframe test binary path: {error}"));
    let world = LoginCompatWorld::new();
    assert!(!world.base.is_skipped());
    world
}

#[given("a routing context with user accounts")]
fn given_users(world: &LoginCompatWorld) -> Result<(), Box<dyn std::error::Error>> {
    world.setup_db(setup_login_db)
}

#[given("a handshake sub-version {sub_version}")]
fn given_sub_version(world: &LoginCompatWorld, sub_version: u16) {
    world.set_handshake_sub_version(HandshakeSubVersion::new(sub_version));
}

#[when("I send a login request with client version {version}")]
fn when_login(world: &LoginCompatWorld, version: u16) {
    world.send_login(ClientVersion::new(version));
}

#[when("I send a login request with invalid credentials and client version {version}")]
fn when_login_with_invalid_credentials(world: &LoginCompatWorld, version: u16) {
    world.send_login_with_credentials(
        ClientVersion::new(version),
        LoginCredentials::new(b"alice", b"wrong-password"),
    );
}

#[then("the login reply includes banner fields")]
fn then_includes_banner_fields(world: &LoginCompatWorld) -> Result<(), Box<dyn std::error::Error>> {
    world.assert_banner_fields(BannerFieldAssertion::ShouldInclude)
}

#[then("the login reply omits banner fields")]
fn then_omits_banner_fields(world: &LoginCompatWorld) -> Result<(), Box<dyn std::error::Error>> {
    world.assert_banner_fields(BannerFieldAssertion::ShouldOmit)
}

#[then("the login reply fails without banner fields")]
fn then_login_fails_without_banner_fields(
    world: &LoginCompatWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    world.assert_login_fails_without_banner_fields()
}

scenarios!(
    "tests/features/wireframe_login_compat.feature",
    fixtures = [world: LoginCompatWorld]
);
