//! Error types for Hotline transaction framing and parameter parsing.

use thiserror::Error;
use tokio::io;

/// Errors that can occur when parsing or writing transactions.
#[derive(Debug, Error)]
pub enum TransactionError {
    /// Frame flags are invalid (must be zero for protocol version 1).
    #[error("invalid flags")] // flags must be zero for v1.8.5
    InvalidFlags,
    /// Payload size exceeds the maximum allowed.
    #[error("payload too large")]
    PayloadTooLarge,
    /// Payload size does not match the header specification.
    #[error("size mismatch")]
    SizeMismatch,
    /// A field identifier appears more than once when not allowed.
    #[error("duplicate field id {0}")]
    DuplicateField(u16),
    /// Buffer is too short to contain the expected data.
    #[error("buffer too short")]
    ShortBuffer,
    /// I/O error occurred during read or write.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// Operation timed out.
    #[error("I/O timeout")]
    Timeout,
}
