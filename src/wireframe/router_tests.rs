//! Tests for wireframe router compatibility hook ordering and strategy dispatch.

use std::{net::SocketAddr, sync::Arc};

use anyhow::anyhow;
use rstest::{fixture, rstest};
use serial_test::serial;
use test_util::{AnyError, TestDb, build_test_db, setup_files_db};
use tokio::runtime::{Builder, Runtime};

use super::{RouteContext, WireframeRouter, compat_spy};
use crate::{
    db::DbPool,
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

const AUTH_STRATEGY_UNKNOWN_DEFAULT: &str = "unknown-default";
const AUTH_STRATEGY_HOTLINE_DEFAULT: &str = "hotline-default";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LoginVersion(u16);

impl LoginVersion {
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "API mirrors domain newtype accessors used throughout tests"
    )]
    fn as_u16(&self) -> u16 { self.0 }
}

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

struct MinimalRouterTestSetup {
    rt: Runtime,
    router: WireframeRouter,
    session: Session,
    peer: SocketAddr,
    messaging: NoopOutboundMessaging,
    pool: DbPool,
}

#[fixture]
fn router_test_setup() -> Result<Option<RouterTestSetup>, AnyError> {
    let rt = runtime()?;
    let test_db = build_test_db(&rt, setup_files_db)
        .map_err(|error| anyhow!("failed to build test database: {error}"))?;
    let Some(test_db) = test_db else {
        return Ok(None);
    };
    let peer = "127.0.0.1:12345".parse()?;
    Ok(Some(RouterTestSetup {
        rt,
        test_db,
        router: test_router(),
        session: Session::default(),
        peer,
        messaging: NoopOutboundMessaging,
    }))
}

#[fixture]
fn minimal_router_setup() -> Result<MinimalRouterTestSetup, AnyError> {
    let rt = runtime()?;
    let peer = "127.0.0.1:12345".parse()?;
    Ok(MinimalRouterTestSetup {
        rt,
        router: test_router(),
        session: Session::default(),
        peer,
        messaging: NoopOutboundMessaging,
        pool: dummy_pool(),
    })
}

fn run_login_test(
    login_version: Option<LoginVersion>,
    expected_auth_strategy: &str,
    setup: &mut RouterTestSetup,
) -> Result<(), AnyError> {
    compat_spy::clear();
    let _ = execute_login(login_version, setup)?;
    let events = compat_spy::take();
    assert_hook_events(&events, expected_auth_strategy)
}

fn execute_login(
    login_version: Option<LoginVersion>,
    setup: &mut RouterTestSetup,
) -> Result<Vec<u8>, AnyError> {
    let version_bytes = login_version.unwrap_or_default().as_u16().to_be_bytes();
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
    if tx.header.error != 0 {
        return Err(anyhow!(
            "login should succeed, got error {}",
            tx.header.error
        ));
    }
    Ok(reply)
}

fn assert_hook_events(
    events: &[compat_spy::HookEvent],
    expected_auth_strategy: &str,
) -> Result<(), AnyError> {
    if events.len() != 3 {
        return Err(anyhow!(
            "expected three compatibility hook events for login, got {events:?}"
        ));
    }
    let expected_on_request = compat_spy::HookEvent::OnRequest {
        tx_type: tx_id(TransactionType::Login),
    };
    if events[0] != expected_on_request {
        return Err(anyhow!(
            "expected first compatibility hook event {:?}, got {:?}",
            expected_on_request,
            events[0]
        ));
    }
    let expected_on_reply = compat_spy::HookEvent::OnReply {
        tx_type: tx_id(TransactionType::Login),
    };
    if events[2] != expected_on_reply {
        return Err(anyhow!(
            "expected final compatibility hook event {:?}, got {:?}",
            expected_on_reply,
            events[2]
        ));
    }
    match &events[1] {
        compat_spy::HookEvent::Dispatch {
            tx_type,
            auth_strategy,
        } if *tx_type == tx_id(TransactionType::Login)
            && *auth_strategy == expected_auth_strategy =>
        {
            Ok(())
        }
        compat_spy::HookEvent::Dispatch {
            tx_type,
            auth_strategy,
        } => Err(anyhow!(
            "unexpected dispatch hook event values (tx_type={tx_type}, \
             auth_strategy={auth_strategy})"
        )),
        unexpected => Err(anyhow!("expected dispatch hook event, got {unexpected:?}")),
    }
}

/// Login hooks fire in order: `on_request` → dispatch → `on_reply`.
#[rstest]
#[serial]
fn login_hook_ordering_is_request_then_dispatch_then_reply(
    router_test_setup: Result<Option<RouterTestSetup>, AnyError>,
) -> Result<(), AnyError> {
    let router_test_setup = router_test_setup?;
    let Some(mut setup) = router_test_setup else {
        return Ok(());
    };
    run_login_test(None, AUTH_STRATEGY_UNKNOWN_DEFAULT, &mut setup)
}

/// Non-login hooks fire in order for authenticated requests.
#[rstest]
#[serial]
fn non_login_hooks_fire_in_order(
    router_test_setup: Result<Option<RouterTestSetup>, AnyError>,
) -> Result<(), AnyError> {
    let router_test_setup = router_test_setup?;
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
    if login_tx.header.error != 0 {
        return Err(anyhow!(
            "login should succeed, got error {}",
            login_tx.header.error
        ));
    }

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
    if tx.header.error != 0 {
        return Err(anyhow!(
            "file list should succeed, got error {}",
            tx.header.error
        ));
    }

    let events = compat_spy::take();
    let expected_events = vec![
        compat_spy::HookEvent::OnRequest {
            tx_type: tx_id(TransactionType::GetFileNameList),
        },
        compat_spy::HookEvent::Dispatch {
            tx_type: tx_id(TransactionType::GetFileNameList),
            auth_strategy: AUTH_STRATEGY_UNKNOWN_DEFAULT,
        },
        compat_spy::HookEvent::OnReply {
            tx_type: tx_id(TransactionType::GetFileNameList),
        },
    ];
    if events != expected_events {
        return Err(anyhow!(
            "unexpected non-login compatibility hook events: got {events:?}, expected \
             {expected_events:?}"
        ));
    }
    Ok(())
}

/// Parse failure does not trigger any compatibility hooks.
#[rstest]
#[serial]
fn parse_failure_does_not_trigger_hooks(
    minimal_router_setup: Result<MinimalRouterTestSetup, AnyError>,
) -> Result<(), AnyError> {
    let mut setup = minimal_router_setup?;
    compat_spy::clear();

    // Truncated frame: only 10 bytes, less than HEADER_LEN.
    let truncated = vec![0u8; 10];
    setup.rt.block_on(setup.router.route(
        &truncated,
        RouteContext {
            peer: setup.peer,
            pool: setup.pool,
            session: &mut setup.session,
            messaging: &setup.messaging,
        },
    ));

    let events = compat_spy::take();
    if !events.is_empty() {
        return Err(anyhow!("no hooks should fire for unparseable input"));
    }
    Ok(())
}

/// Login strategy selection happens after request metadata recording.
#[rstest]
#[serial]
fn first_hotline_login_dispatches_with_hotline_strategy_label(
    router_test_setup: Result<Option<RouterTestSetup>, AnyError>,
) -> Result<(), AnyError> {
    let router_test_setup = router_test_setup?;
    let Some(mut setup) = router_test_setup else {
        return Ok(());
    };
    run_login_test(
        Some(LoginVersion::from(190)),
        AUTH_STRATEGY_HOTLINE_DEFAULT,
        &mut setup,
    )
}
