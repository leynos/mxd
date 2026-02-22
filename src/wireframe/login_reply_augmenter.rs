//! Login reply augmentation abstractions for compatibility guardrails.
//!
//! The guardrail compatibility layer delegates login reply decoration through
//! [`LoginReplyAugmenter`] so reply augmentation can evolve independently from
//! authentication strategy selection.

use crate::{transaction::Transaction, wireframe::compat_policy::ClientCompatibility};

/// Strategy abstraction for login reply decoration.
pub(crate) trait LoginReplyAugmenter: Send + Sync {
    /// Apply compatibility augmentation to a login reply.
    fn augment(&self, reply: &mut Transaction);
}

/// Default login reply augmenter backed by [`ClientCompatibility`].
pub(crate) struct ClientCompatibilityLoginReplyAugmenter<'a> {
    client: &'a ClientCompatibility,
}

impl<'a> ClientCompatibilityLoginReplyAugmenter<'a> {
    /// Construct a login reply augmenter from client compatibility metadata.
    pub(crate) const fn new(client: &'a ClientCompatibility) -> Self { Self { client } }
}

impl LoginReplyAugmenter for ClientCompatibilityLoginReplyAugmenter<'_> {
    fn augment(&self, reply: &mut Transaction) {
        if let Err(error) = self.client.augment_login_reply(reply) {
            tracing::warn!(
                %error,
                client_kind = ?self.client.kind(),
                login_version = ?self.client.login_version(),
                "failed to apply login compatibility extras"
            );
        }
    }
}
