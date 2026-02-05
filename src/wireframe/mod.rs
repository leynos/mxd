//! Wireframe adapter utilities.
//!
//! This module hosts types that bridge Hotline protocol semantics onto the
//! `wireframe` transport layer while keeping the domain crate free of
//! transport-specific dependencies.
//!
//! # Module Structure
//!
//! - [`codec`]: Transaction framing codec (`HotlineTransaction`, `HotlineCodec`)
//! - [`connection`]: Handshake metadata storage
//! - [`context`]: Per-connection state management
//! - [`handshake`]: Preamble success/failure hooks
//! - [`outbound`]: Outbound messaging adapters
//! - [`preamble`]: 12-byte handshake decoder
//! - [`protocol`]: `WireframeProtocol` adapter implementation
//! - [`routes`]: Transaction route handlers
//! - [`route_ids`]: Route identifiers for transaction types

pub mod codec;
pub mod compat;
pub mod compat_policy;
pub mod connection;
pub mod context;
pub mod handshake;
pub mod outbound;
pub mod preamble;
pub mod protocol;
pub mod route_ids;
pub mod routes;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;
