//! Wireframe adapter utilities.
//!
//! This module hosts types that bridge Hotline protocol semantics onto the
//! `wireframe` transport layer while keeping the domain crate free of
//! transport-specific dependencies.

pub mod codec;
pub mod connection;
pub mod handshake;
pub mod preamble;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;
