#![expect(clippy::expect_used, reason = "test assertions")]

//! Behavioural tests for wireframe transaction routing.

use std::{
    cell::{Cell, RefCell},
    net::SocketAddr,
};

use mxd::{
    db::DbPool,
    field_id::FieldId,
    handler::Session,
    transaction::{FrameHeader, HEADER_LEN, Transaction, decode_params, parse_transaction},
    transaction_type::TransactionType,
    wireframe::{routes::process_transaction_bytes, test_helpers::dummy_pool},
};
use rstest::fixture;
use rstest_bdd::assert_step_ok;
use rstest_bdd_macros::{given, scenario, then, when};
use test_util::{
    SetupFn,
    TestDb,
    build_frame,
    build_test_db,
    collect_strings,
    setup_files_db,
    setup_news_categories_root_db,
    setup_news_db,
};
use tokio::runtime::Runtime;

struct RoutingWorld {
    rt: Runtime,
    peer: SocketAddr,
    pool: RefCell<DbPool>,
    db_guard: RefCell<Option<TestDb>>,
    session: RefCell<Session>,
    reply: RefCell<Option<Result<Transaction, String>>>,
    skipped: Cell<bool>,
}

impl RoutingWorld {
    fn new() -> Self {
        let rt = Runtime::new().expect("runtime");
        let peer = "127.0.0.1:12345".parse().expect("valid peer addr");
        Self {
            rt,
            peer,
            pool: RefCell::new(dummy_pool()),
            db_guard: RefCell::new(None),
            session: RefCell::new(Session::default()),
            reply: RefCell::new(None),
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
        let reply = self
            .rt
            .block_on(async { process_transaction_bytes(frame, peer, pool, &mut session).await });
        self.session.replace(session);
        let outcome = parse_transaction(&reply).map_err(|err| err.to_string());
        self.reply.borrow_mut().replace(outcome);
    }

    fn with_reply<T>(&self, f: impl FnOnce(&Transaction) -> T) -> T {
        let reply_ref = self.reply.borrow();
        let Some(reply) = reply_ref.as_ref() else {
            panic!("no reply received");
        };
        let tx = assert_step_ok!(reply.as_ref().map_err(ToString::to_string));
        f(tx)
    }
}

#[fixture]
fn world() -> RoutingWorld {
    let world = RoutingWorld::new();
    debug_assert!(!world.is_skipped(), "world starts active");
    world
}

#[given("a wireframe server handling transactions")]
fn given_server(world: &RoutingWorld) {
    if world.is_skipped() {
        return;
    }
}

#[given("a routing context with user accounts")]
fn given_users(world: &RoutingWorld) { world.setup_db(setup_files_db); }

#[given("a routing context with file access entries")]
fn given_files(world: &RoutingWorld) { world.setup_db(setup_files_db); }

#[given("a routing context with news categories")]
fn given_news_categories(world: &RoutingWorld) { world.setup_db(setup_news_categories_root_db); }

#[given("a routing context with news articles")]
fn given_news_articles(world: &RoutingWorld) { world.setup_db(setup_news_db); }

#[when("I send a transaction with unknown type 65535")]
fn when_unknown_type(world: &RoutingWorld) { world.send(TransactionType::Other(65535), 1, &[]); }

#[when("I send a truncated frame of 10 bytes")]
fn when_truncated(world: &RoutingWorld) { world.send_raw(&[0u8; 10]); }

#[when("I send a transaction with unknown type 65535 and ID {id}")]
fn when_unknown_with_id(world: &RoutingWorld, id: u32) {
    if world.is_skipped() {
        return;
    }
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 65535,
        id,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    let mut header_buf = [0u8; HEADER_LEN];
    header.write_bytes(&mut header_buf);
    world.send_raw(&header_buf);
}

#[given("I send a login transaction for \"{username}\" with password \"{password}\"")]
#[when("I send a login transaction for \"{username}\" with password \"{password}\"")]
fn when_login(world: &RoutingWorld, username: String, password: String) {
    world.send(
        TransactionType::Login,
        10,
        &[
            (FieldId::Login, username.as_bytes()),
            (FieldId::Password, password.as_bytes()),
        ],
    );
}

#[when("I request the file name list")]
fn when_file_list(world: &RoutingWorld) { world.send(TransactionType::GetFileNameList, 11, &[]); }

#[when("I request the news category list")]
fn when_news_categories(world: &RoutingWorld) {
    world.send(TransactionType::NewsCategoryNameList, 12, &[]);
}

#[when("I request the news article list for \"{path}\"")]
fn when_news_articles(world: &RoutingWorld, path: String) {
    world.send(
        TransactionType::NewsArticleNameList,
        13,
        &[(FieldId::NewsPath, path.as_bytes())],
    );
}

#[then("the reply has error code {code}")]
fn then_error_code(world: &RoutingWorld, code: u32) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        assert_eq!(tx.header.error, code);
    });
}

#[then("the reply has transaction ID {id}")]
fn then_transaction_id(world: &RoutingWorld, id: u32) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        assert_eq!(tx.header.id, id);
    });
}

#[then("the reply has transaction type {ty}")]
fn then_transaction_type(world: &RoutingWorld, ty: u16) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        assert_eq!(tx.header.ty, ty);
    });
}

#[then("the session is authenticated")]
fn then_session_authenticated(world: &RoutingWorld) {
    if world.is_skipped() {
        return;
    }
    let session = world.session.borrow();
    assert!(session.user_id.is_some());
}

#[then("the reply lists files \"{first}\" and \"{second}\"")]
fn then_files(world: &RoutingWorld, first: String, second: String) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        let params = assert_step_ok!(decode_params(&tx.payload).map_err(|e| e.to_string()));
        let names =
            assert_step_ok!(collect_strings(&params, FieldId::FileName).map_err(|e| e.to_string()));
        assert_eq!(names, vec![first.as_str(), second.as_str()]);
    });
}

#[then("the reply lists news categories \"{one}\", \"{two}\", and \"{three}\"")]
fn then_categories(world: &RoutingWorld, one: String, two: String, three: String) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        let params = assert_step_ok!(decode_params(&tx.payload).map_err(|e| e.to_string()));
        let mut names = assert_step_ok!(
            collect_strings(&params, FieldId::NewsCategory).map_err(|e| e.to_string())
        );
        names.sort_unstable();
        let mut expected = vec![one.as_str(), two.as_str(), three.as_str()];
        expected.sort_unstable();
        assert_eq!(names, expected);
    });
}

#[then("the reply lists news articles \"{first}\" and \"{second}\"")]
fn then_articles(world: &RoutingWorld, first: String, second: String) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        let params = assert_step_ok!(decode_params(&tx.payload).map_err(|e| e.to_string()));
        let names = assert_step_ok!(
            collect_strings(&params, FieldId::NewsArticle).map_err(|e| e.to_string())
        );
        assert_eq!(names, vec![first.as_str(), second.as_str()]);
    });
}

#[scenario(path = "tests/features/wireframe_routing.feature", index = 0)]
// Unknown-type routing returns ERR_INTERNAL (3) per spec.
fn routes_unknown_type(world: RoutingWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_routing.feature", index = 1)]
fn routes_truncated_frame(world: RoutingWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_routing.feature", index = 2)]
fn preserves_transaction_id(world: RoutingWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_routing.feature", index = 3)]
fn preserves_transaction_type(world: RoutingWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_routing.feature", index = 4)]
fn login_succeeds(world: RoutingWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_routing.feature", index = 5)]
fn file_list_returns_entries(world: RoutingWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_routing.feature", index = 6)]
fn news_categories_listed(world: RoutingWorld) { let _ = world; }

#[scenario(path = "tests/features/wireframe_routing.feature", index = 7)]
fn news_articles_listed(world: RoutingWorld) { let _ = world; }
