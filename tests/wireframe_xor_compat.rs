//! Behavioural tests for XOR compatibility in wireframe routing.

use mxd::{
    field_id::FieldId,
    transaction::Transaction,
    transaction_type::TransactionType,
    wireframe::test_helpers::xor_bytes,
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenarios, then, when};
use test_util::{SetupFn, WireframeBddWorld, build_frame, setup_files_db, setup_news_db};

struct XorWorld {
    base: WireframeBddWorld,
}

impl XorWorld {
    fn new() -> Self {
        Self {
            base: WireframeBddWorld::new(),
        }
    }

    const fn is_skipped(&self) -> bool { self.base.is_skipped() }

    fn setup_db(&self, setup: SetupFn) -> Result<(), Box<dyn std::error::Error>> {
        self.base.setup_db(setup).map_err(Into::into)
    }

    fn authenticate(&self) {
        if self.is_skipped() {
            return;
        }
        self.base.authenticate_default_user(1);
    }

    fn send(&self, ty: TransactionType, id: u32, params: &[(FieldId, &[u8])]) {
        if self.is_skipped() {
            return;
        }
        let frame = match build_frame(ty, id, params) {
            Ok(frame) => frame,
            Err(err) => {
                self.base.set_reply_error(err.to_string());
                return;
            }
        };
        self.base.send_raw(&frame);
    }

    fn with_reply<T>(&self, f: impl FnOnce(&Transaction) -> T) -> T { self.base.with_reply(f) }

    fn is_xor_enabled(&self) -> bool { self.base.is_xor_enabled() }
}

#[fixture]
fn world() -> XorWorld {
    let world = XorWorld::new();
    assert!(!world.is_skipped(), "world starts active");
    world
}

#[given("a routing context with user accounts")]
fn given_users(world: &XorWorld) -> Result<(), Box<dyn std::error::Error>> {
    world.setup_db(setup_files_db)
}

#[given("a routing context with news articles")]
fn given_news(world: &XorWorld) -> Result<(), Box<dyn std::error::Error>> {
    world.setup_db(setup_news_db)?;
    world.authenticate();
    Ok(())
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

#[expect(
    clippy::big_endian_bytes,
    reason = "wire protocol uses big-endian bytes"
)]
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
    assert!(world.is_xor_enabled());
}

scenarios!(
    "tests/features/wireframe_xor_compat.feature",
    fixtures = [world: XorWorld]
);
