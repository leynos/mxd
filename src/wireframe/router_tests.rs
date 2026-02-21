//! Tests for wireframe router compatibility hook ordering and strategy dispatch.

use std::sync::Arc;

use rstest::rstest;
use serial_test::serial;
use test_util::{AnyError, build_test_db, setup_files_db};
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

/// Login hooks fire in order: `on_request` → dispatch → `on_reply`.
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
#[serial]
fn login_hook_ordering_is_request_then_dispatch_then_reply() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let router = test_router();
    let mut session = Session::default();
    let peer = "127.0.0.1:12345".parse()?;
    let messaging = NoopOutboundMessaging;

    compat_spy::clear();

    let frame = build_frame(
        TransactionType::Login,
        1,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    )?;
    let reply = rt.block_on(router.route(
        &frame,
        RouteContext {
            peer,
            pool: test_db.pool(),
            session: &mut session,
            messaging: &messaging,
        },
    ));
    let tx = parse_transaction(&reply)?;
    assert_eq!(tx.header.error, 0, "login should succeed");

    let events = compat_spy::take();
    assert_eq!(
        events,
        vec![
            compat_spy::HookEvent::OnRequest {
                tx_type: tx_id(TransactionType::Login),
            },
            compat_spy::HookEvent::Dispatch {
                tx_type: tx_id(TransactionType::Login),
                auth_strategy: "unknown-default",
            },
            compat_spy::HookEvent::OnReply {
                tx_type: tx_id(TransactionType::Login),
            },
        ],
    );
    Ok(())
}

/// Non-login hooks fire in order for authenticated requests.
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
#[serial]
fn non_login_hooks_fire_in_order() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let router = test_router();
    let mut session = Session::default();
    let peer = "127.0.0.1:12345".parse()?;
    let messaging = NoopOutboundMessaging;

    // Log in first.
    let login_frame = build_frame(
        TransactionType::Login,
        1,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    )?;
    let login_reply = rt.block_on(router.route(
        &login_frame,
        RouteContext {
            peer,
            pool: test_db.pool(),
            session: &mut session,
            messaging: &messaging,
        },
    ));
    let login_tx = parse_transaction(&login_reply)?;
    assert_eq!(login_tx.header.error, 0, "login should succeed");

    // Clear spy after login, then send file list request.
    compat_spy::clear();

    let frame = build_frame(TransactionType::GetFileNameList, 2, &[])?;
    let reply = rt.block_on(router.route(
        &frame,
        RouteContext {
            peer,
            pool: test_db.pool(),
            session: &mut session,
            messaging: &messaging,
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
    let peer = "127.0.0.1:12345".parse().map_err(AnyError::from)?;
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
fn first_hotline_login_dispatches_with_hotline_strategy_label() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let router = test_router();
    let mut session = Session::default();
    let peer = "127.0.0.1:12345".parse()?;
    let messaging = NoopOutboundMessaging;
    let login_version = 190u16.to_be_bytes();

    compat_spy::clear();

    let frame = build_frame(
        TransactionType::Login,
        1,
        &[
            (FieldId::Login, b"alice"),
            (FieldId::Password, b"secret"),
            (FieldId::Version, login_version.as_ref()),
        ],
    )?;
    let reply = rt.block_on(router.route(
        &frame,
        RouteContext {
            peer,
            pool: test_db.pool(),
            session: &mut session,
            messaging: &messaging,
        },
    ));
    let tx = parse_transaction(&reply)?;
    assert_eq!(tx.header.error, 0, "login should succeed");

    let events = compat_spy::take();
    assert_eq!(
        events,
        vec![
            compat_spy::HookEvent::OnRequest {
                tx_type: tx_id(TransactionType::Login),
            },
            compat_spy::HookEvent::Dispatch {
                tx_type: tx_id(TransactionType::Login),
                auth_strategy: "hotline-default",
            },
            compat_spy::HookEvent::OnReply {
                tx_type: tx_id(TransactionType::Login),
            },
        ],
    );
    Ok(())
}
