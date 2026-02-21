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
        auth_strategy::{auth_strategy_for_client, auth_strategy_label},
        compat::XorCompatibility,
        compat_layer::{self, CompatibilityLayer},
        compat_policy::ClientCompatibility,
        login_reply_augmenter::ClientCompatibilityLoginReplyAugmenter,
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
    /// 3. `Command::from_transaction` + auth strategy dispatch.
    /// 4. `CompatibilityLayer::on_reply` via `LoginReplyAugmenter`.
    pub async fn route(&self, frame: &[u8], context: RouteContext<'_>) -> Vec<u8> {
        let RouteContext {
            peer,
            pool,
            session,
            messaging,
        } = context;
        let login_reply_augmenter = ClientCompatibilityLoginReplyAugmenter::new(&self.client);
        let pre_request_layer = CompatibilityLayer::new(
            &self.xor,
            &self.client,
            auth_strategy_for_client(self.client.kind()),
            &login_reply_augmenter,
        );

        let (header, tx_type, cmd) = match Self::prepare_command(frame, peer, &pre_request_layer) {
            Ok(parsed) => parsed,
            Err(reply) => return reply,
        };
        // Select strategy after request hooks so login version metadata affects
        // the first login dispatch.
        let client_kind = self.client.kind();
        let auth_strategy = auth_strategy_for_client(client_kind);
        let compat_layer = CompatibilityLayer::new(
            &self.xor,
            &self.client,
            auth_strategy,
            &login_reply_augmenter,
        );

        #[cfg(test)]
        {
            crate::wireframe::routes::dispatch_spy::record(peer, &header);
            compat_spy::record(compat_spy::HookEvent::Dispatch {
                tx_type: header.ty,
                auth_strategy: auth_strategy_label(client_kind),
            });
        }

        let mut transport = ReplyBuffer::new();
        let command_context = CommandContext {
            peer,
            pool,
            session,
            transport: &mut transport,
            messaging,
        };
        tracing::trace!(
            auth_strategy = auth_strategy_label(client_kind),
            "routing transaction through selected login auth strategy"
        );
        compat_layer
            .process_command(tx_type, cmd, command_context)
            .await
            .map_or_else(
                |e| handle_process_error(peer, &header, e),
                |()| compat_layer::finalize_reply(peer, &header, transport, &compat_layer),
            )
    }

    fn prepare_command(
        frame: &[u8],
        peer: SocketAddr,
        compat_layer: &CompatibilityLayer<'_>,
    ) -> Result<(crate::transaction::FrameHeader, TransactionType, Command), Vec<u8>> {
        let transaction =
            parse_transaction(frame).map_err(|e| handle_parse_error(peer, frame, e))?;
        let header = transaction.header.clone();
        let tx_type = TransactionType::from(header.ty);

        #[cfg(test)]
        compat_spy::record(compat_spy::HookEvent::OnRequest { tx_type: header.ty });

        let request_transaction = compat_layer.on_request(peer, tx_type, transaction)?;
        let command = Command::from_transaction(request_transaction)
            .map_err(|e| handle_command_parse_error(peer, &header, e))?;
        Ok((header, tx_type, command))
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
        Dispatch {
            tx_type: u16,
            auth_strategy: &'static str,
        },
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
#[path = "router_tests.rs"]
mod tests;
