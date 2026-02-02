#![expect(clippy::expect_used, reason = "test assertions")]

//! Behavioural tests for XOR compatibility in wireframe routing.

use std::{
    cell::{Cell, RefCell},
    net::SocketAddr,
    sync::Arc,
};

use mxd::{
    db::DbPool,
    field_id::FieldId,
    handler::Session,
    privileges::Privileges,
    server::outbound::NoopOutboundMessaging,
    transaction::{Transaction, parse_transaction},
    transaction_type::TransactionType,
    wireframe::{
        compat::XorCompatibility,
        routes::{RouteContext, process_transaction_bytes},
    },
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use test_util::{SetupFn, TestDb, build_frame, build_test_db, setup_files_db, setup_news_db};
use tokio::runtime::Runtime;

struct XorWorld {
    rt: Runtime,
    peer: SocketAddr,
    pool: RefCell<DbPool>,
    db_guard: RefCell<Option<TestDb>>,
    session: RefCell<Session>,
    reply: RefCell<Option<Result<Transaction, String>>>,
    compat: Arc<XorCompatibility>,
    skipped: Cell<bool>,
}

impl XorWorld {
    fn new() -> Self {
        let rt = Runtime::new().expect("runtime");
        let peer = "127.0.0.1:12345".parse().expect("valid peer addr");
        Self {
            rt,
            peer,
            pool: RefCell::new(mxd::wireframe::test_helpers::dummy_pool()),
            db_guard: RefCell::new(None),
            session: RefCell::new(Session::default()),
            reply: RefCell::new(None),
            compat: Arc::new(XorCompatibility::disabled()),
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

    fn authenticate(&self) {
        if self.is_skipped() {
            return;
        }
        let mut session = self.session.borrow_mut();
        session.user_id = Some(1);
        session.privileges = Privileges::default_user();
    }

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
        let reply = self.rt.block_on(async {
            process_transaction_bytes(
                frame,
                RouteContext {
                    peer,
                    pool,
                    session: &mut session,
                    messaging: &messaging,
                    compat: compat.as_ref(),
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
        let tx = reply.as_ref().expect("reply should be Ok");
        f(tx)
    }
}

#[fixture]
fn world() -> XorWorld { XorWorld::new() }

fn xor_bytes(data: &[u8]) -> Vec<u8> { data.iter().map(|byte| byte ^ 0xFF).collect() }

#[given("a routing context with user accounts")]
fn given_users(world: &XorWorld) { world.setup_db(setup_files_db); }

#[given("a routing context with news articles")]
fn given_news(world: &XorWorld) {
    world.setup_db(setup_news_db);
    world.authenticate();
}

#[when("I send a login with XOR-encoded credentials")]
fn when_login_xor(world: &XorWorld) {
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

#[when("I send an unknown transaction with XOR-encoded message \"{message}\"")]
fn when_message_xor(world: &XorWorld, message: String) {
    let encoded_message = xor_bytes(message.as_bytes());
    world.send(
        TransactionType::Other(900),
        2,
        &[(FieldId::Data, encoded_message.as_slice())],
    );
}

#[when("I post a news article with XOR-encoded fields")]
fn when_post_news_xor(world: &XorWorld) {
    let encoded_path = xor_bytes(b"General");
    let encoded_title = xor_bytes(b"XorTitle");
    let encoded_flavor = xor_bytes(b"text/plain");
    let encoded_body = xor_bytes(b"xor body");
    let flags = 0i32.to_be_bytes();

    world.send(
        TransactionType::PostNewsArticle,
        3,
        &[
            (FieldId::NewsPath, encoded_path.as_slice()),
            (FieldId::NewsTitle, encoded_title.as_slice()),
            (FieldId::NewsArticleFlags, flags.as_ref()),
            (FieldId::NewsDataFlavor, encoded_flavor.as_slice()),
            (FieldId::NewsArticleData, encoded_body.as_slice()),
        ],
    );
}

#[then("the reply error code is {code}")]
fn then_error_code(world: &XorWorld, code: u32) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        assert_eq!(tx.header.error, code, "unexpected reply error");
    });
}

#[then("XOR compatibility is enabled")]
fn then_xor_enabled(world: &XorWorld) {
    if world.is_skipped() {
        return;
    }
    assert!(world.compat.is_enabled());
}

#[scenario(path = "tests/features/wireframe_xor_compat.feature", index = 0)]
fn xor_login(world: XorWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_xor_compat.feature", index = 1)]
fn xor_message(world: XorWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_xor_compat.feature", index = 2)]
fn xor_news(world: XorWorld) { let _ = world; }
