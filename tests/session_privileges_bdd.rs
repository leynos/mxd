#![expect(clippy::expect_used, reason = "test assertions")]

//! Behavioural tests for session privilege enforcement.

use std::{
    cell::{Cell, RefCell},
    net::SocketAddr,
};

use mxd::{
    db::DbPool,
    field_id::FieldId,
    handler::Session,
    transaction::{Transaction, parse_transaction},
    transaction_type::TransactionType,
    wireframe::{routes::process_transaction_bytes, test_helpers::dummy_pool},
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use test_util::{
    DatabaseUrl,
    SetupFn,
    TestDb,
    build_frame,
    build_test_db,
    setup_files_db,
    setup_news_db,
};
use tokio::runtime::Runtime;

struct PrivilegeWorld {
    rt: Runtime,
    peer: SocketAddr,
    pool: RefCell<DbPool>,
    db_guard: RefCell<Option<TestDb>>,
    session: RefCell<Session>,
    reply: RefCell<Option<Result<Transaction, String>>>,
    skipped: Cell<bool>,
}

impl PrivilegeWorld {
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
        let pool = self.pool.borrow().clone();
        let peer = self.peer;
        let mut session = self.session.replace(Session::default());
        let reply = self
            .rt
            .block_on(async { process_transaction_bytes(&frame, peer, pool, &mut session).await });
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
fn world() -> PrivilegeWorld {
    let world = PrivilegeWorld::new();
    debug_assert!(!world.is_skipped(), "world starts active");
    world
}

/// Setup for tests requiring both users and news data.
fn setup_combined_db(db: DatabaseUrl) -> Result<(), test_util::AnyError> {
    setup_files_db(db.clone())?;
    setup_news_db(db)?;
    Ok(())
}

#[given("a routing context with user accounts and news data")]
fn given_context(world: &PrivilegeWorld) { world.setup_db(setup_combined_db); }

#[given("the session is not authenticated")]
fn given_unauthenticated(world: &PrivilegeWorld) {
    if world.is_skipped() {
        return;
    }
    // Session starts unauthenticated by default
    let session = world.session.borrow();
    assert!(session.user_id.is_none());
}

#[given("I send a login transaction for \"{username}\" with password \"{password}\"")]
fn given_login(world: &PrivilegeWorld, username: String, password: String) {
    world.send(
        TransactionType::Login,
        1,
        &[
            (FieldId::Login, username.as_bytes()),
            (FieldId::Password, password.as_bytes()),
        ],
    );
}

#[when("I request the file name list")]
fn when_file_list(world: &PrivilegeWorld) { world.send(TransactionType::GetFileNameList, 10, &[]); }

#[when("I request the news category list")]
fn when_news_categories(world: &PrivilegeWorld) {
    world.send(TransactionType::NewsCategoryNameList, 11, &[]);
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[when("I post a news article titled \"{title}\" to \"{path}\"")]
fn when_post_article(world: &PrivilegeWorld, title: String, path: String) {
    let flags = 0i32.to_be_bytes();
    world.send(
        TransactionType::PostNewsArticle,
        12,
        &[
            (FieldId::NewsPath, path.as_bytes()),
            (FieldId::NewsTitle, title.as_bytes()),
            (FieldId::NewsArticleFlags, flags.as_ref()),
            (FieldId::NewsDataFlavor, b"text/plain"),
            (FieldId::NewsArticleData, b"test content"),
        ],
    );
}

#[then("the reply has error code {code}")]
fn then_error_code(world: &PrivilegeWorld, code: u32) {
    if world.is_skipped() {
        return;
    }
    world.with_reply(|tx| {
        assert_eq!(tx.header.error, code, "expected error code {code}");
    });
}

#[scenario(path = "tests/features/session_privileges.feature", index = 0)]
fn unauthenticated_file_list(world: PrivilegeWorld) { let _ = world; }

#[scenario(path = "tests/features/session_privileges.feature", index = 1)]
fn authenticated_file_list(world: PrivilegeWorld) { let _ = world; }

#[scenario(path = "tests/features/session_privileges.feature", index = 2)]
fn unauthenticated_news_categories(world: PrivilegeWorld) { let _ = world; }

#[scenario(path = "tests/features/session_privileges.feature", index = 3)]
fn authenticated_news_categories(world: PrivilegeWorld) { let _ = world; }

#[scenario(path = "tests/features/session_privileges.feature", index = 4)]
fn unauthenticated_post_news(world: PrivilegeWorld) { let _ = world; }

#[scenario(path = "tests/features/session_privileges.feature", index = 5)]
fn authenticated_post_news(world: PrivilegeWorld) { let _ = world; }
