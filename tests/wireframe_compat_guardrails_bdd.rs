//! Behavioural tests for wireframe compatibility guardrails.
//!
//! These scenarios verify that [`WireframeRouter`] applies the correct
//! compatibility hooks (banner augmentation, XOR decoding) for different
//! client types.

use std::{
    cell::{Cell, RefCell},
    net::SocketAddr,
    sync::Arc,
};

use mxd::{
    commands::ERR_NOT_AUTHENTICATED,
    db::DbPool,
    field_id::FieldId,
    handler::Session,
    presence::PresenceRegistry,
    server::outbound::NoopOutboundMessaging,
    transaction::{Transaction, decode_params, parse_transaction},
    transaction_type::TransactionType,
    wireframe::{
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        connection::HandshakeMetadata,
        router::{RouteContext, WireframeRouter},
        test_helpers::{dummy_pool, xor_bytes},
    },
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenarios, then, when};
use test_util::{SetupFn, TestDb, build_frame, build_test_db, setup_files_db};
use tokio::runtime::Runtime;

struct GuardrailWorld {
    runtime: Runtime,
    peer: SocketAddr,
    pool: RefCell<DbPool>,
    db_guard: RefCell<Option<TestDb>>,
    session: RefCell<Session>,
    reply: RefCell<Option<Result<Transaction, String>>>,
    router: RefCell<WireframeRouter>,
    skipped: Cell<bool>,
}

impl GuardrailWorld {
    fn new() -> Self {
        let peer = "127.0.0.1:12345"
            .parse()
            .unwrap_or_else(|err| panic!("failed to parse fixture peer address: {err}"));
        let runtime =
            Runtime::new().unwrap_or_else(|err| panic!("failed to create tokio runtime: {err}"));
        let router =
            Self::build_router(XorCompatibility::disabled(), &HandshakeMetadata::default());
        Self {
            runtime,
            peer,
            pool: RefCell::new(dummy_pool()),
            db_guard: RefCell::new(None),
            session: RefCell::new(Session::default()),
            reply: RefCell::new(None),
            router: RefCell::new(router),
            skipped: Cell::new(false),
        }
    }

    fn build_router(xor: XorCompatibility, handshake: &HandshakeMetadata) -> WireframeRouter {
        WireframeRouter::new(
            Arc::new(xor),
            Arc::new(ClientCompatibility::from_handshake(handshake)),
        )
    }

    const fn is_skipped(&self) -> bool { self.skipped.get() }

    fn setup_db(&self, setup: SetupFn) {
        if self.is_skipped() {
            return;
        }
        let db = match build_test_db(&self.runtime, setup) {
            Ok(Some(db)) => db,
            Ok(None) => {
                self.skipped.set(true);
                return;
            }
            Err(err) => panic!("failed to set up database: {err}"),
        };
        self.pool.replace(db.pool());
        self.db_guard.replace(Some(db));
        self.session.replace(Session::default());
    }

    fn set_router(&self, router: WireframeRouter) { self.router.replace(router); }

    fn send(&self, ty: TransactionType, id: u32, params: &[(FieldId, &[u8])]) {
        if self.is_skipped() {
            return;
        }
        let frame = match build_frame(ty, id, params) {
            Ok(frame) => frame,
            Err(err) => {
                self.reply.borrow_mut().replace(Err(err.to_string()));
                return;
            }
        };
        let pool = self.pool.borrow().clone();
        let peer = self.peer;
        let mut session = self.session.borrow().clone();
        let messaging = NoopOutboundMessaging;
        let presence = PresenceRegistry::default();
        let router = self.router.borrow();
        let reply = self.runtime.block_on(router.route(
            &frame,
            RouteContext {
                peer,
                pool,
                session: &mut session,
                messaging: &messaging,
                presence: &presence,
            },
        ));
        self.session.replace(session);
        let outcome = parse_transaction(&reply).map_err(|err| err.to_string());
        self.reply.borrow_mut().replace(outcome);
    }

    fn with_reply<T>(&self, f: impl FnOnce(&Transaction) -> T) -> T {
        let reply_ref = self.reply.borrow();
        let Some(reply) = reply_ref.as_ref() else {
            panic!("no reply received");
        };
        let Ok(tx) = reply.as_ref() else {
            panic!("reply should be Ok");
        };
        f(tx)
    }
}

#[fixture]
fn world() -> GuardrailWorld {
    let world = GuardrailWorld::new();
    debug_assert!(!world.is_skipped(), "world starts active");
    world
}

#[given("a wireframe server with user accounts")]
fn given_server_with_users(world: &GuardrailWorld) { world.setup_db(setup_files_db); }

#[given("a logged-in client")]
fn given_logged_in_client(world: &GuardrailWorld) {
    world.send(
        TransactionType::Login,
        1,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    );
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        assert_eq!(tx.header.error, 0, "setup login should succeed");
    });
}

#[expect(
    clippy::big_endian_bytes,
    reason = "test fixture uses explicit network byte order payload"
)]
#[when("a Hotline 1.9 client logs in")]
fn when_hotline_19_login(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }
    // Hotline 1.9: sub_version 0 (default), login version >= 190.
    let version_bytes = 190u16.to_be_bytes();
    world.send(
        TransactionType::Login,
        1,
        &[
            (FieldId::Login, b"alice"),
            (FieldId::Password, b"secret"),
            (FieldId::Version, version_bytes.as_slice()),
        ],
    );
}

#[when("a SynHX client logs in")]
fn when_synhx_login(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }
    // SynHX: handshake sub_version = 2.
    let handshake = HandshakeMetadata {
        sub_version: 2,
        ..HandshakeMetadata::default()
    };
    world.set_router(GuardrailWorld::build_router(
        XorCompatibility::disabled(),
        &handshake,
    ));
    world.send(
        TransactionType::Login,
        1,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    );
}

#[expect(
    clippy::big_endian_bytes,
    reason = "test fixture uses explicit network byte order payload"
)]
#[when("a SynHX client logs in with Hotline 1.9 version")]
fn when_synhx_login_with_hotline_19_version(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }
    let handshake = HandshakeMetadata {
        sub_version: 2,
        ..HandshakeMetadata::default()
    };
    world.set_router(GuardrailWorld::build_router(
        XorCompatibility::disabled(),
        &handshake,
    ));
    let version_bytes = 190u16.to_be_bytes();
    world.send(
        TransactionType::Login,
        1,
        &[
            (FieldId::Login, b"alice"),
            (FieldId::Password, b"secret"),
            (FieldId::Version, version_bytes.as_slice()),
        ],
    );
}

#[expect(
    clippy::big_endian_bytes,
    reason = "test fixture uses explicit network byte order payload"
)]
#[when("a client logs in with invalid credentials and Hotline 1.9 version")]
fn when_invalid_hotline_19_login(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }
    let version_bytes = 190u16.to_be_bytes();
    world.send(
        TransactionType::Login,
        1,
        &[
            (FieldId::Login, b"alice"),
            (FieldId::Password, b"wrong-password"),
            (FieldId::Version, version_bytes.as_slice()),
        ],
    );
}

#[when("a client sends a XOR-encoded login")]
fn when_xor_login(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }
    let encoded_login = xor_bytes(b"alice");
    let encoded_password = xor_bytes(b"secret");
    world.send(
        TransactionType::Login,
        1,
        &[
            (FieldId::Login, encoded_login.as_slice()),
            (FieldId::Password, encoded_password.as_slice()),
        ],
    );
}

#[when("the client requests the file name list")]
fn when_file_list(world: &GuardrailWorld) { world.send(TransactionType::GetFileNameList, 2, &[]); }

#[expect(clippy::expect_used, reason = "test assertion")]
fn assert_banner_fields(world: &GuardrailWorld, should_include: bool) {
    if world.is_skipped() {
        return;
    }

    world.with_reply(|tx| {
        assert_eq!(tx.header.error, 0, "login should succeed");
        let params = decode_params(&tx.payload).expect("valid reply payload");
        if should_include {
            assert!(
                params.iter().any(|(id, _)| *id == FieldId::BannerId),
                "reply should contain BannerId field"
            );
            assert!(
                params.iter().any(|(id, _)| *id == FieldId::ServerName),
                "reply should contain ServerName field"
            );
            return;
        }
        assert!(
            !params.iter().any(|(id, _)| *id == FieldId::BannerId),
            "reply should not contain BannerId field"
        );
        assert!(
            !params.iter().any(|(id, _)| *id == FieldId::ServerName),
            "reply should not contain ServerName field"
        );
    });
}

#[then("the login reply includes banner fields")]
fn then_includes_banner(world: &GuardrailWorld) { assert_banner_fields(world, true); }

#[then("the login reply does not include banner fields")]
fn then_omits_banner(world: &GuardrailWorld) { assert_banner_fields(world, false); }

#[then("the login succeeds")]
fn then_login_succeeds(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        assert_eq!(tx.header.error, 0, "login should succeed");
    });
}

#[expect(clippy::expect_used, reason = "test assertion")]
#[then("the login fails without banner fields")]
fn then_login_fails_without_banner(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }

    world.with_reply(|tx| {
        assert_eq!(
            tx.header.error, ERR_NOT_AUTHENTICATED,
            "login should fail with not-authenticated error"
        );
        let params = decode_params(&tx.payload).expect("valid reply payload");
        assert!(
            !params.iter().any(|(id, _)| *id == FieldId::BannerId),
            "reply should not contain BannerId field"
        );
        assert!(
            !params.iter().any(|(id, _)| *id == FieldId::ServerName),
            "reply should not contain ServerName field"
        );
    });
}

#[then("XOR encoding is enabled for the connection")]
fn then_xor_enabled(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }
    let router = world.router.borrow();
    assert!(router.xor().is_enabled(), "XOR encoding should be enabled");
}

#[expect(clippy::expect_used, reason = "test assertion")]
#[then("the reply contains file names")]
fn then_reply_has_files(world: &GuardrailWorld) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        assert_eq!(tx.header.error, 0, "file list should succeed");
        let params = decode_params(&tx.payload).expect("valid reply payload");
        assert!(
            params.iter().any(|(id, _)| *id == FieldId::FileName),
            "reply should contain FileName entries"
        );
    });
}

scenarios!(
    "tests/features/wireframe_compat_guardrails.feature",
    fixtures = [world: GuardrailWorld]
);
