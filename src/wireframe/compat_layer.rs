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

    /// Decode transaction payload bytes when the transaction type
    /// carries payload.
    fn decode_payload(
        &self,
        peer: SocketAddr,
        tx_type: TransactionType,
        transaction: Transaction,
    ) -> Result<Transaction, Vec<u8>> {
        if !quirk_registry::hook_expectation(tx_type).decodes_payload {
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
        if quirk_registry::hook_expectation(tx_type).records_login_version
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

pub(crate) mod quirk_registry {
    //! Per-transaction-type hook expectations.
    //!
    //! This module is the single source of truth for which
    //! compatibility hooks apply to each transaction type. The
    //! [`CompatibilityLayer`](super::CompatibilityLayer) consults
    //! [`hook_expectation`] at runtime, and tests verify that the
    //! spy recordings match the registry for every known variant.
    //!
    //! Because [`hook_expectation`] is a `const fn` with a `match`,
    //! the compiler optimises it to the same machine code as inline
    //! checks.

    use crate::transaction_type::TransactionType;

    /// Describes which compatibility hooks are active for a
    /// transaction type.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct HookExpectation {
        /// Whether XOR payload decoding is attempted.
        pub(crate) decodes_payload: bool,
        /// Whether the login version is recorded from the payload.
        pub(crate) records_login_version: bool,
        /// Whether the reply may be augmented with client-specific
        /// extras (e.g. banner fields for Hotline 1.9).
        pub(crate) augments_login_reply: bool,
    }

    /// Return the hook expectation for a given transaction type.
    ///
    /// This is the authoritative mapping consulted by the
    /// compatibility layer at runtime.
    #[must_use]
    pub(crate) const fn hook_expectation(tx_type: TransactionType) -> HookExpectation {
        match tx_type {
            TransactionType::Login => HookExpectation {
                decodes_payload: true,
                records_login_version: true,
                augments_login_reply: true,
            },
            TransactionType::GetFileNameList
            | TransactionType::DownloadBanner
            | TransactionType::GetUserNameList => HookExpectation {
                decodes_payload: false,
                records_login_version: false,
                augments_login_reply: false,
            },
            _ => HookExpectation {
                decodes_payload: true,
                records_login_version: false,
                augments_login_reply: false,
            },
        }
    }

    #[cfg(test)]
    mod tests {
        use rstest::rstest;

        use super::{HookExpectation, hook_expectation};
        use crate::transaction_type::TransactionType;

        /// All known transaction types for parameterised testing.
        const KNOWN_VARIANTS: &[TransactionType] = &[
            TransactionType::Error,
            TransactionType::Login,
            TransactionType::Agreement,
            TransactionType::Agreed,
            TransactionType::GetFileNameList,
            TransactionType::DownloadBanner,
            TransactionType::GetUserNameList,
            TransactionType::UserAccess,
            TransactionType::NewsCategoryNameList,
            TransactionType::NewsArticleNameList,
            TransactionType::NewsArticleData,
            TransactionType::PostNewsArticle,
        ];

        /// Registry `decodes_payload` agrees with
        /// `TransactionType::allows_payload()` for every known
        /// variant.
        #[rstest]
        fn registry_agrees_with_allows_payload() {
            for &tx_type in KNOWN_VARIANTS {
                let expected = tx_type.allows_payload();
                let actual = hook_expectation(tx_type).decodes_payload;
                assert_eq!(actual, expected, "{tx_type}: decodes_payload mismatch");
            }
        }

        /// Only `Login` records the login version.
        #[rstest]
        fn only_login_records_version() {
            for &tx_type in KNOWN_VARIANTS {
                let expectation = hook_expectation(tx_type);
                if tx_type == TransactionType::Login {
                    assert!(
                        expectation.records_login_version,
                        "Login should record login version"
                    );
                } else {
                    assert!(
                        !expectation.records_login_version,
                        "{tx_type}: should not record login version"
                    );
                }
            }
        }

        /// Only `Login` may augment the reply.
        #[rstest]
        fn only_login_augments_reply() {
            for &tx_type in KNOWN_VARIANTS {
                let expectation = hook_expectation(tx_type);
                if tx_type == TransactionType::Login {
                    assert!(
                        expectation.augments_login_reply,
                        "Login should augment login reply"
                    );
                } else {
                    assert!(
                        !expectation.augments_login_reply,
                        "{tx_type}: should not augment login reply"
                    );
                }
            }
        }

        /// `Other(n)` types decode payload but do not record
        /// version or augment the reply.
        #[rstest]
        #[case::unknown_900(900)]
        #[case::unknown_1(1)]
        #[case::unknown_65535(65535)]
        fn other_types_decode_payload_only(#[case] id: u16) {
            let tx_type = TransactionType::Other(id);
            assert_eq!(
                hook_expectation(tx_type),
                HookExpectation {
                    decodes_payload: true,
                    records_login_version: false,
                    augments_login_reply: false,
                },
                "Other({id}): unexpected expectation"
            );
        }
    }
}
