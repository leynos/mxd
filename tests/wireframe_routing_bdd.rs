//! Behavioural tests for wireframe transaction routing against the binary.

use mxd::{
    commands::ERR_NOT_AUTHENTICATED,
    field_id::FieldId,
    transaction::{FrameHeader, HEADER_LEN, Transaction, decode_params},
    transaction_type::TransactionType,
};
use rstest::fixture;
use rstest_bdd::assert_step_ok;
use rstest_bdd_macros::{given, scenarios, then, when};
use test_util::{
    AnyError,
    SetupFn,
    WireframeBddWorld,
    build_frame,
    collect_strings,
    ensure_server_binary_env,
    setup_files_db,
    setup_news_categories_root_db,
    setup_news_db,
};

struct RoutingWorld {
    base: WireframeBddWorld,
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "keeps setup callback signature aligned with fixture setup helpers"
)]
fn noop_setup(_: test_util::DatabaseUrl) -> Result<(), AnyError> { Ok(()) }

impl RoutingWorld {
    const fn new() -> Self {
        Self {
            base: WireframeBddWorld::new(),
        }
    }

    const fn is_skipped(&self) -> bool { self.base.is_skipped() }

    fn setup_db(&self, setup: SetupFn) -> Result<(), AnyError> { self.base.setup_db(setup) }

    fn authenticate(&self) { self.base.authenticate_default_user(1); }

    fn send(&self, ty: TransactionType, id: u32, params: &[(FieldId, &[u8])]) {
        if self.is_skipped() {
            return;
        }
        let frame = match build_frame(ty, id, params) {
            Ok(frame) => frame,
            Err(error) => {
                self.base.set_reply_error(error.to_string());
                return;
            }
        };
        self.base.send_raw(&frame);
    }

    fn send_raw(&self, frame: &[u8]) {
        if self.is_skipped() {
            return;
        }
        self.base.send_raw(frame);
    }

    fn with_reply<T>(&self, f: impl FnOnce(&Transaction) -> T) -> T { self.base.with_reply(f) }

    fn assert_reply_contains_two_strings(&self, field_id: FieldId, first: &str, second: &str) {
        self.with_reply(|tx| {
            let params = assert_step_ok!(decode_params(&tx.payload).map_err(|e| e.to_string()));
            let names =
                assert_step_ok!(collect_strings(&params, field_id).map_err(|e| e.to_string()));
            assert_eq!(names, vec![first, second]);
        });
    }

    fn send_header(&self, header: &FrameHeader) {
        let mut header_buf = [0u8; HEADER_LEN];
        header.write_bytes(&mut header_buf);
        self.send_raw(&header_buf);
    }
}

#[fixture]
fn world() -> RoutingWorld {
    ensure_server_binary_env(env!("CARGO_BIN_EXE_mxd-wireframe-server"))
        .unwrap_or_else(|error| panic!("failed to configure wireframe test binary path: {error}"));
    RoutingWorld::new()
}

#[given("a wireframe server handling transactions")]
fn given_server(world: &RoutingWorld) -> Result<(), AnyError> {
    debug_assert!(!world.is_skipped(), "world starts active");
    world.setup_db(noop_setup)
}

#[given("a routing context with user accounts")]
fn given_users(world: &RoutingWorld) -> Result<(), AnyError> { world.setup_db(setup_files_db) }

#[given("a routing context with file access entries")]
fn given_files(world: &RoutingWorld) -> Result<(), AnyError> { world.setup_db(setup_files_db) }

#[given("a routing context with news categories")]
fn given_news_categories(world: &RoutingWorld) -> Result<(), AnyError> {
    world.setup_db(setup_news_categories_root_db)?;
    world.authenticate();
    Ok(())
}

#[given("a routing context with news articles")]
fn given_news_articles(world: &RoutingWorld) -> Result<(), AnyError> {
    world.setup_db(setup_news_db)?;
    world.authenticate();
    Ok(())
}

#[when("I send a transaction with unknown type 65535")]
fn when_unknown_type(world: &RoutingWorld) { world.send(TransactionType::Other(65535), 1, &[]); }

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
    world.send_header(&header);
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
    world.send(TransactionType::GetFileNameList, 14, &[]);
    world.with_reply(|tx| {
        assert_ne!(
            tx.header.error, ERR_NOT_AUTHENTICATED,
            "session should remain authenticated after successful login",
        );
    });
}

#[then("the reply lists files \"{first}\" and \"{second}\"")]
fn then_files(world: &RoutingWorld, first: String, second: String) {
    if world.is_skipped() {
        return;
    }
    world.assert_reply_contains_two_strings(FieldId::FileName, &first, &second);
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
    world.assert_reply_contains_two_strings(FieldId::NewsArticle, &first, &second);
}

scenarios!(
    "tests/features/wireframe_routing.feature",
    fixtures = [world: RoutingWorld]
);
