//! Public routing entrypoint for wireframe transactions.
//!
//! `WireframeRouter` is the sole `pub` routing entrypoint. It
//! embeds a [`CompatibilityLayer`] that orchestrates request and
//! reply compatibility hooks, ensuring every routed transaction
//! passes through the same compatibility pipeline.

use std::{net::SocketAddr, sync::Arc};

use crate::{
    commands::{Command, CommandContext},
    db::DbPool,
    handler::Session,
    server::outbound::{OutboundMessaging, ReplyBuffer},
    transaction::parse_transaction,
    transaction_type::TransactionType,
    wireframe::{
        compat::XorCompatibility,
        compat_layer::{self, CompatibilityLayer},
        compat_policy::ClientCompatibility,
        routes::{handle_command_parse_error, handle_parse_error, handle_process_error},
    },
};

/// Public routing entrypoint for wireframe transactions.
///
/// The router owns the XOR and client compatibility state for the
/// connection and creates a [`CompatibilityLayer`] on each call to
/// [`route`](Self::route). This guarantees that every transaction
/// passes through the same request and reply hooks regardless of
/// which code path dispatches it.
#[derive(Clone)]
pub struct WireframeRouter {
    xor: Arc<XorCompatibility>,
    client: Arc<ClientCompatibility>,
}

impl WireframeRouter {
    /// Construct a router with the given compatibility state.
    #[must_use]
    pub const fn new(xor: Arc<XorCompatibility>, client: Arc<ClientCompatibility>) -> Self {
        Self { xor, client }
    }

    /// Route a raw transaction frame through the compatibility
    /// pipeline and domain command dispatcher.
    ///
    /// This is the sole public routing entrypoint. The full
    /// pipeline is:
    ///
    /// 1. Parse the frame.
    /// 2. `CompatibilityLayer::on_request` (XOR decode + login version recording).
    /// 3. `Command::from_transaction` + `process_with_outbound`.
    /// 4. `CompatibilityLayer::on_reply` (login augmentation).
    pub async fn route(&self, frame: &[u8], context: RouteContext<'_>) -> Vec<u8> {
        let RouteContext {
            peer,
            pool,
            session,
            messaging,
        } = context;
        let compat_layer = CompatibilityLayer::new(&self.xor, &self.client);

        let transaction = match parse_transaction(frame) {
            Ok(tx) => tx,
            Err(e) => return handle_parse_error(peer, frame, e),
        };
        let header = transaction.header.clone();
        let tx_type = TransactionType::from(header.ty);

        #[cfg(test)]
        compat_spy::record(compat_spy::HookEvent::OnRequest { tx_type: header.ty });

        let tx = match compat_layer.on_request(peer, tx_type, transaction) {
            Ok(tx) => tx,
            Err(reply) => return reply,
        };

        let cmd = match Command::from_transaction(tx) {
            Ok(cmd) => cmd,
            Err(e) => return handle_command_parse_error(peer, &header, e),
        };

        #[cfg(test)]
        crate::wireframe::routes::dispatch_spy::record(peer, &header);
        #[cfg(test)]
        compat_spy::record(compat_spy::HookEvent::Dispatch { tx_type: header.ty });

        let mut transport = ReplyBuffer::new();
        let command_context = CommandContext {
            peer,
            pool,
            session,
            transport: &mut transport,
            messaging,
        };
        match cmd.process_with_outbound(command_context).await {
            Ok(()) => compat_layer::finalize_reply(peer, &header, transport, &compat_layer),
            Err(e) => handle_process_error(peer, &header, e),
        }
    }

    /// Read-only access to the XOR compatibility state.
    #[must_use]
    pub fn xor(&self) -> &XorCompatibility { self.xor.as_ref() }
}

/// Reduced routing context for the domain dispatch path.
///
/// Unlike the previous `RouteContext`, this struct does not carry
/// compatibility state — that is owned by the
/// [`WireframeRouter`] and its embedded [`CompatibilityLayer`].
pub struct RouteContext<'a> {
    /// Remote peer socket address.
    pub peer: SocketAddr,
    /// Database connection pool.
    pub pool: DbPool,
    /// Mutable session state for the connection.
    pub session: &'a mut Session,
    /// Outbound messaging adapter for push notifications.
    pub messaging: &'a dyn OutboundMessaging,
}

#[cfg(test)]
pub(crate) mod compat_spy {
    //! Captures compatibility hook events for ordering assertions.

    use std::cell::RefCell;

    /// A recorded hook event with the transaction type.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) enum HookEvent {
        /// `CompatibilityLayer::on_request` was called.
        OnRequest { tx_type: u16 },
        /// Command dispatch occurred.
        Dispatch { tx_type: u16 },
        /// `CompatibilityLayer::on_reply` was called.
        OnReply { tx_type: u16 },
    }

    thread_local! {
        static EVENTS: RefCell<Vec<HookEvent>> = const { RefCell::new(Vec::new()) };
    }

    pub(crate) fn record(event: HookEvent) {
        EVENTS.with(|events| events.borrow_mut().push(event));
    }

    pub(crate) fn take() -> Vec<HookEvent> {
        EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
    }

    pub(crate) fn clear() { EVENTS.with(|events| events.borrow_mut().clear()); }
}

#[cfg(test)]
mod tests {
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
}
