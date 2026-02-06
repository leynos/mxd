//! Behavioural tests for login compatibility gating.

use std::{
    cell::{Cell, RefCell},
    net::SocketAddr,
    sync::Arc,
};

use mxd::{
    db::DbPool,
    field_id::FieldId,
    handler::Session,
    server::outbound::NoopOutboundMessaging,
    transaction::{Transaction, decode_params, parse_transaction},
    transaction_type::TransactionType,
    wireframe::{
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        connection::HandshakeMetadata,
        routes::{RouteContext, process_transaction_bytes},
    },
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use test_util::{SetupFn, TestDb, build_frame, build_test_db, setup_files_db};
use tokio::runtime::Runtime;

struct LoginCompatWorld {
    rt: Runtime,
    peer: SocketAddr,
    pool: RefCell<DbPool>,
    db_guard: RefCell<Option<TestDb>>,
    session: RefCell<Session>,
    reply: RefCell<Option<Result<Transaction, String>>>,
    compat: Arc<XorCompatibility>,
    client_compat: RefCell<Arc<ClientCompatibility>>,
    skipped: Cell<bool>,
}

impl LoginCompatWorld {
    fn new() -> Self {
        #[expect(clippy::expect_used, reason = "runtime setup in test harness")]
        let rt = Runtime::new().expect("runtime");
        #[expect(clippy::expect_used, reason = "fixed test fixture address parses")]
        let peer = "127.0.0.1:12345".parse().expect("valid peer addr");
        let handshake = HandshakeMetadata::default();
        Self {
            rt,
            peer,
            pool: RefCell::new(mxd::wireframe::test_helpers::dummy_pool()),
            db_guard: RefCell::new(None),
            session: RefCell::new(Session::default()),
            reply: RefCell::new(None),
            compat: Arc::new(XorCompatibility::disabled()),
            client_compat: RefCell::new(Arc::new(ClientCompatibility::from_handshake(&handshake))),
            skipped: Cell::new(false),
        }
    }

    const fn is_skipped(&self) -> bool { self.skipped.get() }

    fn setup_db(&self, setup: SetupFn) {
        if self.is_skipped() {
            return;
        }
        let db = match build_test_db(&self.rt, setup) {
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

    fn set_handshake_sub_version(&self, sub_version: u16) {
        if self.is_skipped() {
            return;
        }
        let handshake = HandshakeMetadata {
            sub_version,
            ..HandshakeMetadata::default()
        };
        self.client_compat
            .replace(Arc::new(ClientCompatibility::from_handshake(&handshake)));
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
                self.reply.borrow_mut().replace(Err(err.to_string()));
                return;
            }
        };
        self.send_raw(&frame);
    }

    fn send_raw(&self, frame: &[u8]) {
        if self.is_skipped() {
            return;
        }
        let pool = self.pool.borrow().clone();
        let peer = self.peer;
        let mut session = self.session.replace(Session::default());
        let messaging = NoopOutboundMessaging;
        let compat = Arc::clone(&self.compat);
        let client_compat = Arc::clone(&self.client_compat.borrow());
        let reply = self.rt.block_on(async {
            process_transaction_bytes(
                frame,
                RouteContext {
                    peer,
                    pool,
                    session: &mut session,
                    messaging: &messaging,
                    compat: compat.as_ref(),
                    client_compat: client_compat.as_ref(),
                },
            )
            .await
        });
        self.session.replace(session);
        let outcome = parse_transaction(&reply).map_err(|err| err.to_string());
        self.reply.borrow_mut().replace(outcome);
    }

    fn with_reply<T>(&self, f: impl FnOnce(&Transaction) -> T) -> T {
        let reply_ref = self.reply.borrow();
        let Some(reply) = reply_ref.as_ref() else {
            panic!("no reply received");
        };
        let tx = match reply.as_ref() {
            Ok(tx) => tx,
            Err(error) => panic!("reply parse failed: {error}"),
        };
        f(tx)
    }

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
            #[expect(
                clippy::expect_used,
                reason = "behavioural test asserts decodable reply"
            )]
            let params = decode_params(&tx.payload).expect("decode reply params");
            let banner_field = params.iter().find(|(id, _)| *id == FieldId::BannerId);
            let server_field = params.iter().find(|(id, _)| *id == FieldId::ServerName);

            if should_include {
                #[expect(clippy::expect_used, reason = "behavioural test asserts banner field")]
                let banner = banner_field.expect("missing banner id");
                #[expect(clippy::expect_used, reason = "behavioural test asserts server field")]
                let server = server_field.expect("missing server name");
                assert_eq!(banner.1, [0u8, 0u8, 0u8, 0u8]);
                assert_eq!(server.1, b"mxd");
            } else {
                assert!(banner_field.is_none());
                assert!(server_field.is_none());
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
fn given_users(world: &LoginCompatWorld) { world.setup_db(setup_files_db); }

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

#[scenario(path = "tests/features/wireframe_login_compat.feature", index = 0)]
fn hotline_85_login(world: LoginCompatWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_login_compat.feature", index = 1)]
fn hotline_19_login(world: LoginCompatWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_login_compat.feature", index = 2)]
fn synhx_login(world: LoginCompatWorld) { let _ = world; }
