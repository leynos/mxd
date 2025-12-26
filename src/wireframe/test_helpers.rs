//! Shared test utilities for wireframe adapter tests.
//!
//! These helpers keep handshake and transaction-related test plumbing in one
//! place so unit and behaviour suites reuse the same encoding logic.
//!
//! This module is only available when running tests or when the `test-support`
//! feature is enabled.

#![allow(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use std::time::Duration;

use diesel_async::pooled_connection::{AsyncDieselConnectionManager, bb8::Pool};
use tokio::io::AsyncReadExt;

use crate::{
    db::{DbConnection, DbPool},
    protocol::{HANDSHAKE_LEN, REPLY_LEN},
    transaction::{FrameHeader, HEADER_LEN},
};

/// Create a lightweight database pool for tests that don't require real
/// database connections.
///
/// The pool is configured with an invalid connection string and will not
/// attempt to connect. This is suitable for tests that only need a `DbPool`
/// type without actually executing queries.
#[must_use]
pub fn dummy_pool() -> DbPool {
    let manager =
        AsyncDieselConnectionManager::<DbConnection>::new("postgres://example.invalid/mxd-test");
    Pool::builder()
        .max_size(1)
        .min_idle(Some(0))
        .idle_timeout(None::<Duration>)
        .max_lifetime(None::<Duration>)
        .test_on_check_out(false)
        .build_unchecked(manager)
}

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

/// Build a transaction frame buffer from a header and payload.
///
/// The returned buffer contains the serialised 20-byte header followed by the
/// payload bytes.
#[must_use]
pub fn transaction_bytes(header: &FrameHeader, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_LEN + payload.len());
    let mut hdr_buf = [0u8; HEADER_LEN];
    header.write_bytes(&mut hdr_buf);
    buf.extend_from_slice(&hdr_buf);
    buf.extend_from_slice(payload);
    buf
}

/// Build fragmented transaction frames from a header and payload.
///
/// Each fragment contains a copy of the header with `data_size` adjusted to
/// reflect the chunk size. The first fragment receives the initial portion of
/// the payload, and subsequent fragments receive the remaining chunks.
///
/// # Arguments
///
/// * `header` - Base header (`total_size` should match payload length)
///
/// Errors returned by wireframe test helper builders.
#[derive(Debug)]
pub enum FragmentError {
    /// Calculated slice bounds exceeded the payload length.
    SliceOutOfBounds,
    /// Fragment length could not be represented as `u32`.
    Length(std::num::TryFromIntError),
}

impl From<std::num::TryFromIntError> for FragmentError {
    fn from(err: std::num::TryFromIntError) -> Self { Self::Length(err) }
}

impl std::fmt::Display for FragmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SliceOutOfBounds => write!(f, "payload slice bounds exceeded payload length"),
            Self::Length(err) => write!(f, "fragment length overflow: {err}"),
        }
    }
}

impl std::error::Error for FragmentError {}

/// * `payload` - Complete payload to fragment
/// * `fragment_size` - Maximum data bytes per fragment
///
/// # Errors
///
/// Returns an error if any chunk length exceeds `u32::MAX` or if calculated
/// slice bounds fall outside the payload.
pub fn fragmented_transaction_bytes(
    header: &FrameHeader,
    payload: &[u8],
    fragment_size: usize,
) -> Result<Vec<Vec<u8>>, FragmentError> {
    debug_assert_eq!(
        header.total_size as usize,
        payload.len(),
        "header.total_size must match payload.len()"
    );

    let mut fragments = Vec::new();
    let mut offset = 0usize;

    while offset < payload.len() {
        let end = (offset + fragment_size).min(payload.len());
        let chunk = payload
            .get(offset..end)
            .ok_or(FragmentError::SliceOutOfBounds)?;

        let mut frag_header = header.clone();
        frag_header.data_size = u32::try_from(chunk.len())?;

        fragments.push(transaction_bytes(&frag_header, chunk));
        offset = end;
    }

    // Handle empty payload case
    if fragments.is_empty() {
        let mut frag_header = header.clone();
        frag_header.data_size = 0;
        fragments.push(transaction_bytes(&frag_header, &[]));
    }

    Ok(fragments)
}

/// Build fragmented transaction frames where the continuation header has a
/// mismatched field.
///
/// Creates a two-fragment transaction where the second frame has a different
/// transaction ID than the first, which should be rejected by the decoder.
///
/// # Errors
///
/// Returns an error if any chunk length exceeds `u32::MAX`.
pub fn mismatched_continuation_bytes() -> Result<Vec<u8>, FragmentError> {
    let total_size = 2000u32;
    let first_chunk = 1000usize;

    let payload = vec![0u8; total_size as usize];

    // First fragment
    let first_header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size,
        data_size: u32::try_from(first_chunk)?,
    };

    // Second fragment with mismatched ID
    let second_header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 999, // Different ID — this should trigger header mismatch
        error: 0,
        total_size,
        data_size: u32::try_from(total_size as usize - first_chunk)?,
    };

    let first_slice = payload
        .get(..first_chunk)
        .ok_or(FragmentError::SliceOutOfBounds)?;
    let second_slice = payload
        .get(first_chunk..)
        .ok_or(FragmentError::SliceOutOfBounds)?;

    let mut bytes = transaction_bytes(&first_header, first_slice);
    bytes.extend(transaction_bytes(&second_header, second_slice));

    Ok(bytes)
}
