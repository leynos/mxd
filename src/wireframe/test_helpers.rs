//! Shared test utilities for wireframe adapter tests.
//!
//! These helpers keep handshake-related test plumbing in one place so unit and
//! behaviour suites reuse the same encoding logic.

#![allow(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use tokio::io::AsyncReadExt;

use crate::protocol::{HANDSHAKE_LEN, REPLY_LEN};

/// Build a Hotline preamble buffer for tests.
#[must_use]
pub fn preamble_bytes(
    protocol: [u8; 4],
    sub_protocol: [u8; 4],
    version: u16,
    sub_version: u16,
) -> [u8; HANDSHAKE_LEN] {
    let mut buf = [0u8; HANDSHAKE_LEN];
    buf[0..4].copy_from_slice(&protocol);
    buf[4..8].copy_from_slice(&sub_protocol);
    buf[8..10].copy_from_slice(&version.to_be_bytes());
    buf[10..12].copy_from_slice(&sub_version.to_be_bytes());
    buf
}

/// Receive a single Hotline handshake reply from the stream.
///
/// # Errors
///
/// Returns an error if the stream cannot supply the full reply buffer.
pub async fn recv_reply(
    stream: &mut tokio::net::TcpStream,
) -> Result<[u8; REPLY_LEN], std::io::Error> {
    let mut buf = [0u8; REPLY_LEN];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}
