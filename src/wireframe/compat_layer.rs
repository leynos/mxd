//! Compatibility layer orchestrating request and reply hooks.
//!
//! `CompatibilityLayer` centralizes command-dispatch and reply-side
//! compatibility logic that was previously scattered across inline calls in the
//! routing module. Request-side hooks are handled by
//! [`RequestCompatibility`] so login version metadata can be recorded before
//! auth strategy selection.

use std::net::SocketAddr;

use crate::{
    commands::{Command, CommandContext, CommandError},
    transaction::{FrameHeader, Transaction},
    transaction_type::TransactionType,
    wireframe::{
        auth_strategy::AuthStrategy,
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        login_reply_augmenter::LoginReplyAugmenter,
        routes::reply_builder::ReplyBuilder,
    },
};

/// Error code for internal server failures.
const ERR_INTERNAL: u32 = 3;

/// Centralized compatibility hooks for the transaction lifecycle.
///
/// The router constructs this layer after request hooks have run and client
/// metadata has been recorded. It exposes two hooks:
///
/// - [`process_command`](Self::process_command) — routes login command execution through
///   [`AuthStrategy`].
/// - [`on_reply`](Self::on_reply) — augments the reply with client-specific extras (e.g. banner
///   fields for Hotline 1.9).
pub(crate) struct CompatibilityLayer<'a> {
    auth_strategy: &'a dyn AuthStrategy,
    login_reply_augmenter: &'a dyn LoginReplyAugmenter,
}

impl<'a> CompatibilityLayer<'a> {
    /// Construct a new compatibility layer from borrowed state.
    pub(crate) const fn new(
        auth_strategy: &'a dyn AuthStrategy,
        login_reply_augmenter: &'a dyn LoginReplyAugmenter,
    ) -> Self {
        Self {
            auth_strategy,
            login_reply_augmenter,
        }
    }

    /// Process a parsed command through login auth strategy or default dispatch.
    ///
    /// Login commands are delegated to the selected [`AuthStrategy`]. All
    /// other commands keep the existing command dispatch behaviour.
    pub(crate) async fn process_command(
        &self,
        tx_type: TransactionType,
        command: Command,
        context: CommandContext<'_>,
    ) -> Result<(), CommandError> {
        if tx_type == TransactionType::Login {
            self.auth_strategy.authenticate(command, context).await
        } else {
            command.process_with_outbound(context).await
        }
    }

    /// Apply reply-side compatibility hooks.
    ///
    /// Augments the reply transaction with client-specific extras
    /// (e.g. banner fields for Hotline 1.9 clients).
    pub(crate) fn on_reply(&self, reply: &mut Transaction) {
        self.login_reply_augmenter.augment(reply);
    }
}

/// Request-side compatibility hooks evaluated before auth strategy selection.
pub(crate) struct RequestCompatibility<'a> {
    xor: &'a XorCompatibility,
    client: &'a ClientCompatibility,
}

impl<'a> RequestCompatibility<'a> {
    /// Construct request-side compatibility from shared connection state.
    pub(crate) const fn new(xor: &'a XorCompatibility, client: &'a ClientCompatibility) -> Self {
        Self { xor, client }
    }

    /// Decode payload compatibility and record login version metadata.
    pub(crate) fn on_request(
        &self,
        peer: SocketAddr,
        tx_type: TransactionType,
        transaction: Transaction,
    ) -> Result<Transaction, Vec<u8>> {
        let tx = decode_payload_for_request(self.xor, peer, tx_type, transaction)?;
        record_login_version_for_request(self.client, tx_type, &tx.payload);
        Ok(tx)
    }
}

/// Decode transaction payload bytes when the transaction type carries payload.
fn decode_payload_for_request(
    xor: &XorCompatibility,
    peer: SocketAddr,
    tx_type: TransactionType,
    transaction: Transaction,
) -> Result<Transaction, Vec<u8>> {
    if tx_type.bypass_payload_decode() {
        return Ok(transaction);
    }
    let decoded_payload = match xor.decode_payload(&transaction.payload) {
        Ok(payload) => payload,
        Err(error) => {
            return Err(ReplyBuilder::from_header(peer, &transaction.header)
                .command_parse_error(error, ERR_INTERNAL));
        }
    };
    Ok(Transaction {
        payload: decoded_payload,
        ..transaction
    })
}

/// Record login version metadata for the compatibility policy.
fn record_login_version_for_request(
    client: &ClientCompatibility,
    tx_type: TransactionType,
    payload: &[u8],
) {
    if tx_type == TransactionType::Login
        && let Err(error) = client.record_login_payload(payload)
    {
        tracing::warn!(%error, "failed to record login version from payload");
    }
}

/// Finalize a successful command reply by extracting the reply from
/// the transport buffer and applying compatibility augmentation.
///
/// If the transport has no reply, returns error bytes for the
/// missing-reply case. Otherwise applies
/// [`CompatibilityLayer::on_reply`] and serializes the result.
pub(crate) fn finalize_reply(
    peer: SocketAddr,
    header: &FrameHeader,
    mut transport: crate::server::outbound::ReplyBuffer,
    compat_layer: &CompatibilityLayer<'_>,
) -> Vec<u8> {
    transport.take_reply().map_or_else(
        || ReplyBuilder::from_header(peer, header).missing_reply(ERR_INTERNAL),
        |mut reply| {
            #[cfg(test)]
            crate::wireframe::router::compat_spy::record(
                crate::wireframe::router::compat_spy::HookEvent::OnReply { tx_type: header.ty },
            );
            compat_layer.on_reply(&mut reply);
            reply.to_bytes()
        },
    )
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        sync::atomic::{AtomicU32, Ordering},
    };

    use tokio::runtime::Builder;

    use super::{CompatibilityLayer, decode_payload_for_request};
    use crate::{
        commands::{Command, CommandContext},
        handler::Session,
        server::outbound::{NoopOutboundMessaging, ReplyBuffer},
        transaction::{FrameHeader, Transaction},
        transaction_type::TransactionType,
        wireframe::{
            auth_strategy::{AuthStrategy, AuthStrategyFuture},
            compat::XorCompatibility,
            login_reply_augmenter::LoginReplyAugmenter,
            test_helpers::dummy_pool,
        },
    };

    struct SpyAuthStrategy {
        login_calls: AtomicU32,
    }

    impl SpyAuthStrategy {
        const fn new() -> Self {
            Self {
                login_calls: AtomicU32::new(0),
            }
        }
    }

    impl AuthStrategy for SpyAuthStrategy {
        fn authenticate<'a>(
            &self,
            command: Command,
            context: CommandContext<'a>,
        ) -> AuthStrategyFuture<'a> {
            self.login_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { command.process_with_outbound(context).await })
        }
    }

    struct SpyReplyAugmenter {
        calls: AtomicU32,
    }

    impl SpyReplyAugmenter {
        const fn new() -> Self {
            Self {
                calls: AtomicU32::new(0),
            }
        }
    }

    impl LoginReplyAugmenter for SpyReplyAugmenter {
        fn augment(&self, _reply: &mut Transaction) { self.calls.fetch_add(1, Ordering::SeqCst); }
    }

    fn header(tx_type: TransactionType) -> FrameHeader {
        FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: u16::from(tx_type),
            id: 1,
            error: 0,
            total_size: 0,
            data_size: 0,
        }
    }

    fn run_auth_strategy_test(tx_type: TransactionType, expected_auth_calls: u32) {
        let auth = SpyAuthStrategy::new();
        let augmenter = SpyReplyAugmenter::new();
        let layer = CompatibilityLayer::new(&auth, &augmenter);
        let mut session = Session::default();
        let mut transport = ReplyBuffer::new();
        let messaging = NoopOutboundMessaging;
        let context = CommandContext {
            peer: "127.0.0.1:12345".parse().expect("valid loopback socket"),
            pool: dummy_pool(),
            session: &mut session,
            transport: &mut transport,
            messaging: &messaging,
        };
        let command = Command::Unknown {
            header: header(tx_type),
        };

        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime builds");
        runtime
            .block_on(layer.process_command(tx_type, command, context))
            .expect("command succeeds");

        assert_eq!(
            auth.login_calls.load(Ordering::SeqCst),
            expected_auth_calls,
            "unexpected auth strategy call count for {tx_type:?}"
        );
    }

    #[test]
    fn login_commands_use_auth_strategy() { run_auth_strategy_test(TransactionType::Login, 1); }

    #[test]
    fn non_login_commands_bypass_auth_strategy() {
        run_auth_strategy_test(TransactionType::GetFileNameList, 0);
    }

    #[test]
    fn file_list_payload_bypasses_param_decode() {
        let peer: SocketAddr = "127.0.0.1:12345".parse().expect("valid loopback socket");
        let xor = XorCompatibility::disabled();
        let transaction = Transaction {
            header: header(TransactionType::GetFileNameList),
            payload: vec![0xca, 0x00, 0x02, 0x00, 0x01],
        };

        let decoded = decode_payload_for_request(
            &xor,
            peer,
            TransactionType::GetFileNameList,
            transaction.clone(),
        )
        .expect("file-list payload should bypass parameter decode");

        assert_eq!(decoded.payload, transaction.payload);
    }

    #[test]
    fn on_reply_calls_login_reply_augmenter() {
        let auth = SpyAuthStrategy::new();
        let augmenter = SpyReplyAugmenter::new();
        let layer = CompatibilityLayer::new(&auth, &augmenter);
        let mut reply = Transaction {
            header: header(TransactionType::Login),
            payload: Vec::new(),
        };

        layer.on_reply(&mut reply);

        assert_eq!(
            augmenter.calls.load(Ordering::SeqCst),
            1,
            "reply augmenter should run once"
        );
    }
}
