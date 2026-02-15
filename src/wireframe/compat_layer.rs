//! Compatibility layer orchestrating request and reply hooks.
//!
//! `CompatibilityLayer` centralises the per-transaction compatibility
//! logic that was previously scattered across inline calls in the
//! routing module. It owns references to the XOR and client
//! compatibility state and exposes `on_request` and `on_reply`
//! methods that the router calls at well-defined points in the
//! transaction lifecycle.

use std::net::SocketAddr;

use crate::{
    transaction::{FrameHeader, Transaction},
    transaction_type::TransactionType,
    wireframe::{
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        routes::reply_builder::ReplyBuilder,
    },
};

/// Error code for internal server failures.
const ERR_INTERNAL: u32 = 3;

/// Centralised compatibility hooks for the transaction lifecycle.
///
/// The layer is constructed once per connection and shared by the
/// router. It exposes two hooks:
///
/// - [`on_request`](Self::on_request) — decodes XOR-encoded payloads and records the login version
///   when applicable.
/// - [`on_reply`](Self::on_reply) — augments the reply with client-specific extras (e.g. banner
///   fields for Hotline 1.9).
pub(crate) struct CompatibilityLayer<'a> {
    xor: &'a XorCompatibility,
    client: &'a ClientCompatibility,
}

impl<'a> CompatibilityLayer<'a> {
    /// Construct a new compatibility layer from borrowed state.
    pub(crate) const fn new(xor: &'a XorCompatibility, client: &'a ClientCompatibility) -> Self {
        Self { xor, client }
    }

    /// Apply request-side compatibility hooks.
    ///
    /// 1. Decode XOR-encoded payload when the transaction type carries payload.
    /// 2. Record the login version from the payload when the transaction is a login request.
    ///
    /// Returns the (potentially decoded) transaction on success, or
    /// error reply bytes when payload decoding fails.
    pub(crate) fn on_request(
        &self,
        peer: SocketAddr,
        tx_type: TransactionType,
        transaction: Transaction,
    ) -> Result<Transaction, Vec<u8>> {
        let tx = self.decode_payload(peer, tx_type, transaction)?;
        self.record_login_version(tx_type, &tx.payload);
        Ok(tx)
    }

    /// Apply reply-side compatibility hooks.
    ///
    /// Augments the reply transaction with client-specific extras
    /// (e.g. banner fields for Hotline 1.9 clients).
    pub(crate) fn on_reply(&self, reply: &mut Transaction) {
        if let Err(error) = self.client.augment_login_reply(reply) {
            tracing::warn!(
                %error,
                client_kind = ?self.client.kind(),
                login_version = ?self.client.login_version(),
                "failed to apply login compatibility extras"
            );
        }
    }

    /// Read-only access to the XOR compatibility state.
    #[cfg(test)]
    #[expect(dead_code, reason = "used by spy-based tests in later stages")]
    pub(crate) const fn xor(&self) -> &XorCompatibility { self.xor }

    /// Read-only access to the client compatibility policy.
    #[cfg(test)]
    #[expect(dead_code, reason = "used by spy-based tests in later stages")]
    pub(crate) const fn client(&self) -> &ClientCompatibility { self.client }

    /// Decode transaction payload bytes when the transaction type
    /// carries payload.
    fn decode_payload(
        &self,
        peer: SocketAddr,
        tx_type: TransactionType,
        transaction: Transaction,
    ) -> Result<Transaction, Vec<u8>> {
        if !tx_type.allows_payload() {
            return Ok(transaction);
        }
        let decoded_payload = match self.xor.decode_payload(&transaction.payload) {
            Ok(payload) => payload,
            Err(e) => {
                return Err(ReplyBuilder::from_header(peer, &transaction.header)
                    .command_parse_error(e, ERR_INTERNAL));
            }
        };
        Ok(Transaction {
            payload: decoded_payload,
            ..transaction
        })
    }

    /// Record login version metadata for the compatibility policy.
    fn record_login_version(&self, tx_type: TransactionType, payload: &[u8]) {
        if tx_type == TransactionType::Login
            && let Err(error) = self.client.record_login_payload(payload)
        {
            tracing::warn!(%error, "failed to record login version from payload");
        }
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
            compat_layer.on_reply(&mut reply);
            reply.to_bytes()
        },
    )
}
