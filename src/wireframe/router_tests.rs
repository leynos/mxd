//! Tests for wireframe router compatibility hook ordering and strategy dispatch.

use std::{net::SocketAddr, sync::Arc};

use rstest::{fixture, rstest};
use serial_test::serial;
use test_util::{AnyError, TestDb, build_test_db, setup_files_db};
use tokio::runtime::{Builder, Runtime};

use super::{RouteContext, WireframeRouter, compat_spy};
use crate::{
    field_id::FieldId,
    handler::Session,
    server::outbound::NoopOutboundMessaging,
    transaction::parse_transaction,
    transaction_type::TransactionType,
    wireframe::{
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        connection::HandshakeMetadata,
        test_helpers::{build_frame, dummy_pool},
    },
};

fn runtime() -> Result<Runtime, AnyError> {
    Ok(Builder::new_current_thread().enable_all().build()?)
}

fn test_router() -> WireframeRouter {
    WireframeRouter::new(
        Arc::new(XorCompatibility::disabled()),
        Arc::new(ClientCompatibility::from_handshake(
            &HandshakeMetadata::default(),
        )),
    )
}

fn tx_id(tx_type: TransactionType) -> u16 { u16::from(tx_type) }

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LoginVersion(u16);

impl From<u16> for LoginVersion {
    fn from(value: u16) -> Self { Self(value) }
}

struct RouterTestSetup {
    rt: Runtime,
    test_db: TestDb,
    router: WireframeRouter,
    session: Session,
    peer: SocketAddr,
    messaging: NoopOutboundMessaging,
}

#[fixture]
fn router_test_setup() -> Option<RouterTestSetup> {
    let rt = runtime().unwrap_or_else(|error| panic!("failed to build runtime: {error}"));
    let test_db = build_test_db(&rt, setup_files_db)
        .unwrap_or_else(|error| panic!("failed to build test database: {error}"))?;
    let peer = "127.0.0.1:12345"
        .parse()
        .unwrap_or_else(|error| panic!("failed to parse peer address: {error}"));
    Some(RouterTestSetup {
        rt,
        test_db,
        router: test_router(),
        session: Session::default(),
        peer,
        messaging: NoopOutboundMessaging,
    })
}

#[expect(clippy::panic_in_result_fn, reason = "test helper assertions")]
fn run_login_test(
    login_version: Option<LoginVersion>,
    expected_auth_strategy: &str,
    setup: &mut RouterTestSetup,
) -> Result<(), AnyError> {
    compat_spy::clear();

    let version_bytes = login_version.unwrap_or_default().0.to_be_bytes();
    let mut fields: Vec<(FieldId, &[u8])> = vec![
        (FieldId::Login, b"alice".as_ref()),
        (FieldId::Password, b"secret".as_ref()),
    ];
    if login_version.is_some() {
        fields.push((FieldId::Version, version_bytes.as_ref()));
    }

    let frame = build_frame(TransactionType::Login, 1, &fields)?;
    let reply = setup.rt.block_on(setup.router.route(
        &frame,
        RouteContext {
            peer: setup.peer,
            pool: setup.test_db.pool(),
            session: &mut setup.session,
            messaging: &setup.messaging,
        },
    ));
    let tx = parse_transaction(&reply)?;
    assert_eq!(tx.header.error, 0, "login should succeed");

    let events = compat_spy::take();
    assert_eq!(
        events.len(),
        3,
        "expected three compatibility hook events for login, got {events:?}"
    );
    assert_eq!(
        events[0],
        compat_spy::HookEvent::OnRequest {
            tx_type: tx_id(TransactionType::Login),
        }
    );
    assert_eq!(
        events[2],
        compat_spy::HookEvent::OnReply {
            tx_type: tx_id(TransactionType::Login),
        }
    );
    match &events[1] {
        compat_spy::HookEvent::Dispatch {
            tx_type,
            auth_strategy,
        } => {
            assert_eq!(*tx_type, tx_id(TransactionType::Login));
            assert_eq!(*auth_strategy, expected_auth_strategy);
        }
        unexpected => panic!("expected dispatch hook event, got {unexpected:?}"),
    }
    Ok(())
}

/// Login hooks fire in order: `on_request` → dispatch → `on_reply`.
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
#[serial]
fn login_hook_ordering_is_request_then_dispatch_then_reply(
    router_test_setup: Option<RouterTestSetup>,
) -> Result<(), AnyError> {
    let Some(mut setup) = router_test_setup else {
        return Ok(());
    };
    let result = run_login_test(None, "unknown-default", &mut setup);
    assert!(result.is_ok(), "login test should succeed: {result:?}");
    result
}

/// Non-login hooks fire in order for authenticated requests.
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
#[serial]
fn non_login_hooks_fire_in_order(
    router_test_setup: Option<RouterTestSetup>,
) -> Result<(), AnyError> {
    let Some(mut setup) = router_test_setup else {
        return Ok(());
    };

    // Log in first.
    let login_frame = build_frame(
        TransactionType::Login,
        1,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    )?;
    let login_reply = setup.rt.block_on(setup.router.route(
        &login_frame,
        RouteContext {
            peer: setup.peer,
            pool: setup.test_db.pool(),
            session: &mut setup.session,
            messaging: &setup.messaging,
        },
    ));
    let login_tx = parse_transaction(&login_reply)?;
    assert_eq!(login_tx.header.error, 0, "login should succeed");

    // Clear spy after login, then send file list request.
    compat_spy::clear();

    let frame = build_frame(TransactionType::GetFileNameList, 2, &[])?;
    let reply = setup.rt.block_on(setup.router.route(
        &frame,
        RouteContext {
            peer: setup.peer,
            pool: setup.test_db.pool(),
            session: &mut setup.session,
            messaging: &setup.messaging,
        },
    ));
    let tx = parse_transaction(&reply)?;
    assert_eq!(tx.header.error, 0, "file list should succeed");

    let events = compat_spy::take();
    assert_eq!(
        events,
        vec![
            compat_spy::HookEvent::OnRequest {
                tx_type: tx_id(TransactionType::GetFileNameList),
            },
            compat_spy::HookEvent::Dispatch {
                tx_type: tx_id(TransactionType::GetFileNameList),
                auth_strategy: "unknown-default",
            },
            compat_spy::HookEvent::OnReply {
                tx_type: tx_id(TransactionType::GetFileNameList),
            },
        ],
    );
    Ok(())
}

/// Parse failure does not trigger any compatibility hooks.
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
#[serial]
fn parse_failure_does_not_trigger_hooks() -> Result<(), AnyError> {
    let router = test_router();
    let mut session = Session::default();
    let peer = "127.0.0.1:12345".parse()?;
    let messaging = NoopOutboundMessaging;
    let pool = dummy_pool();

    compat_spy::clear();

    // Truncated frame: only 10 bytes, less than HEADER_LEN.
    let truncated = vec![0u8; 10];
    let rt = runtime()?;
    rt.block_on(router.route(
        &truncated,
        RouteContext {
            peer,
            pool,
            session: &mut session,
            messaging: &messaging,
        },
    ));

    let events = compat_spy::take();
    assert!(
        events.is_empty(),
        "no hooks should fire for unparseable input"
    );
    Ok(())
}

/// Login strategy selection happens after request metadata recording.
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
#[serial]
fn first_hotline_login_dispatches_with_hotline_strategy_label(
    router_test_setup: Option<RouterTestSetup>,
) -> Result<(), AnyError> {
    let Some(mut setup) = router_test_setup else {
        return Ok(());
    };
    let result = run_login_test(Some(LoginVersion::from(190)), "hotline-default", &mut setup);
    assert!(result.is_ok(), "login test should succeed: {result:?}");
    result
}
