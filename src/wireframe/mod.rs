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
//! - [`connection_handler`]: Custom connection handler using `HotlineCodec`
//! - [`context`]: Per-connection state management
//! - [`handshake`]: Preamble success/failure hooks
//! - [`preamble`]: 12-byte handshake decoder
//! - [`protocol`]: `WireframeProtocol` adapter implementation
//! - [`routes`]: Transaction route handlers

pub mod codec;
pub mod connection;
pub mod connection_handler;
pub mod context;
pub mod handshake;
pub mod preamble;
pub mod protocol;
pub mod routes;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;
