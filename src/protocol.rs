//! Constants and helpers for the wire protocol.
//!
//! The protocol module defines handshake sequences and utility functions for
//! talking to Hotline clients. It contains the numeric constants used by the
//! framing layer and basic handshake implementation.
use std::time::Duration;

use thiserror::Error;
use tokio::io::{self, AsyncWriteExt};

/// Number of bytes in the client handshake message.
pub const HANDSHAKE_LEN: usize = 12;
/// Number of bytes in the server handshake reply.
pub const REPLY_LEN: usize = 8;
/// Fixed protocol identifier used in the Hotline protocol.
pub const PROTOCOL_ID: &[u8; 4] = b"TRTP";
/// Protocol version supported by this server.
pub const VERSION: u16 = 1;
/// Hotline client version code used in login replies.
pub const CLIENT_VERSION: u16 = 0x0097; // 151

/// Handshake reply code for success.
pub const HANDSHAKE_OK: u32 = 0;
/// Error code for an invalid protocol identifier.
pub const HANDSHAKE_ERR_INVALID: u32 = 1;
/// Error code for an unsupported protocol version.
pub const HANDSHAKE_ERR_UNSUPPORTED_VERSION: u32 = 2;
/// Error code when the handshake times out.
pub const HANDSHAKE_ERR_TIMEOUT: u32 = 3;

/// Timeout for reading the client handshake.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Parsed handshake information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handshake {
    /// Application-specific sub-protocol identifier.
    pub sub_protocol: u32,
    /// Protocol version number.
    pub version: u16,
    /// Application-defined sub-version number.
    pub sub_version: u16,
}

/// Errors that can occur when parsing a handshake.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HandshakeError {
    #[error("invalid protocol id")]
    InvalidProtocol,
    #[error("unsupported version {0}")]
    UnsupportedVersion(u16),
}

/// Parse the 12-byte client handshake message.
///
/// # Errors
/// Returns an error if the message is malformed or the version is unsupported.
#[must_use = "handle the result"]
pub fn parse_handshake(buf: &[u8; HANDSHAKE_LEN]) -> Result<Handshake, HandshakeError> {
    if &buf[0..4] != PROTOCOL_ID {
        return Err(HandshakeError::InvalidProtocol);
    }
    let sub_protocol = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let version = u16::from_be_bytes([buf[8], buf[9]]);
    let sub_version = u16::from_be_bytes([buf[10], buf[11]]);
    if version != VERSION {
        return Err(HandshakeError::UnsupportedVersion(version));
    }
    Ok(Handshake {
        sub_protocol,
        version,
        sub_version,
    })
}

/// Convert a [`HandshakeError`] into a numeric error code for clients.
#[must_use = "handle the result"]
pub fn handshake_error_code(err: &HandshakeError) -> u32 {
    match err {
        HandshakeError::InvalidProtocol => HANDSHAKE_ERR_INVALID,
        HandshakeError::UnsupportedVersion(_) => HANDSHAKE_ERR_UNSUPPORTED_VERSION,
    }
}

/// Write the handshake reply with the provided error code.
///
/// The reply consists of the protocol identifier followed by a 32-bit
/// error code. [`HANDSHAKE_OK`] indicates success, while the other
/// `HANDSHAKE_ERR_*` constants specify why the handshake failed.
///
/// # Errors
/// Returns any I/O error encountered while sending the reply.
#[must_use = "handle the result"]
pub async fn write_handshake_reply<W>(writer: &mut W, error_code: u32) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut buf = [0u8; REPLY_LEN];
    buf[0..4].copy_from_slice(PROTOCOL_ID);
    buf[4..8].copy_from_slice(&error_code.to_be_bytes());
    writer.write_all(&buf).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_handshake() {
        let mut buf = [0u8; HANDSHAKE_LEN];
        buf[0..4].copy_from_slice(PROTOCOL_ID);
        buf[8..10].copy_from_slice(&VERSION.to_be_bytes());
        let hs = parse_handshake(&buf).unwrap();
        assert_eq!(
            hs,
            Handshake {
                sub_protocol: 0,
                version: VERSION,
                sub_version: 0
            }
        );
    }

    #[test]
    fn reject_invalid_protocol() {
        let mut buf = [0u8; HANDSHAKE_LEN];
        buf[0..4].copy_from_slice(b"WRNG");
        buf[8..10].copy_from_slice(&VERSION.to_be_bytes());
        assert!(matches!(
            parse_handshake(&buf),
            Err(HandshakeError::InvalidProtocol)
        ));
        assert_eq!(
            handshake_error_code(&HandshakeError::InvalidProtocol),
            HANDSHAKE_ERR_INVALID
        );
    }

    #[test]
    fn reject_bad_version() {
        let mut buf = [0u8; HANDSHAKE_LEN];
        buf[0..4].copy_from_slice(PROTOCOL_ID);
        buf[8..10].copy_from_slice(&2u16.to_be_bytes());
        assert!(matches!(
            parse_handshake(&buf),
            Err(HandshakeError::UnsupportedVersion(2))
        ));
        assert_eq!(
            handshake_error_code(&HandshakeError::UnsupportedVersion(2)),
            HANDSHAKE_ERR_UNSUPPORTED_VERSION
        );
    }
}
