//! Wireframe adapter utilities.
//!
//! This module hosts types that bridge Hotline protocol semantics onto the
//! `wireframe` transport layer while keeping the domain crate free of
//! transport-specific dependencies.
//!
//! # Module Structure
//!
//! - [`auth_strategy`]: Login authentication strategy abstractions
//! - [`codec`]: Transaction framing codec (`HotlineTransaction`, `HotlineCodec`)
//! - [`compat_policy`]: Client compatibility policy for login reply gating
//! - [`connection`]: Handshake metadata storage
//! - [`context`]: Per-connection state management
//! - [`handshake`]: Preamble success/failure hooks
//! - [`login_reply_augmenter`]: Login reply augmentation abstractions
//! - [`outbound`]: Outbound messaging adapters
//! - [`preamble`]: 12-byte handshake decoder
//! - [`protocol`]: `WireframeProtocol` adapter implementation
//! - [`routes`]: Transaction route handlers
//! - [`route_ids`]: Route identifiers for transaction types

pub(crate) mod auth_strategy;
pub mod codec;
pub mod compat;
pub(crate) mod compat_layer;
pub mod compat_policy;
pub mod connection;
pub mod context;
pub mod handshake;
pub(crate) mod login_reply_augmenter;
pub(crate) mod message_assembly;
pub mod outbound;
pub mod preamble;
pub mod protocol;
pub mod route_ids;
pub mod router;
pub mod routes;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;
