//! Frame-level Hotline transaction encoding and decoding helpers.
//!
//! This module owns the fixed 20-byte header format, plus low-level read/write
//! helpers for single physical frames. Higher-level reassembly and streaming
//! live in sibling modules.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use std::time::Duration;

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::timeout,
};

use super::{
    HEADER_LEN,
    IO_TIMEOUT,
    MAX_FRAME_DATA,
    MAX_PAYLOAD_SIZE,
    errors::TransactionError,
    params::validate_payload,
};

async fn io_with_timeout<F, T>(timeout_dur: Duration, operation: F) -> Result<T, TransactionError>
where
    F: std::future::Future<Output = std::io::Result<T>>,
{
    timeout(timeout_dur, operation)
        .await
        .map_err(|_| TransactionError::Timeout)?
        .map_err(Into::into)
}

async fn read_timeout_exact<R: AsyncRead + Unpin>(
    r: &mut R,
    buf: &mut [u8],
    timeout_dur: Duration,
) -> Result<(), TransactionError> {
    io_with_timeout(timeout_dur, r.read_exact(buf))
        .await
        .map(|_| ())
}

async fn write_timeout_all<W: AsyncWrite + Unpin>(
    w: &mut W,
    buf: &[u8],
    timeout_dur: Duration,
) -> Result<(), TransactionError> {
    io_with_timeout(timeout_dur, w.write_all(buf)).await
}

/// Read a big-endian `u32` from the provided byte slice.
///
/// # Errors
/// Returns an error if `buf` is shorter than four bytes.
#[must_use = "handle the result"]
#[expect(clippy::indexing_slicing, reason = "length is checked before indexing")]
pub fn read_u32(buf: &[u8]) -> Result<u32, TransactionError> {
    if buf.len() < 4 {
        return Err(TransactionError::ShortBuffer);
    }
    Ok(u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]))
}

/// Read a big-endian `u16` from the provided byte slice.
///
/// # Errors
/// Returns an error if `buf` is shorter than two bytes.
#[must_use = "handle the result"]
#[expect(clippy::indexing_slicing, reason = "length is checked before indexing")]
pub fn read_u16(buf: &[u8]) -> Result<u16, TransactionError> {
    if buf.len() < 2 {
        return Err(TransactionError::ShortBuffer);
    }
    Ok(u16::from_be_bytes([buf[0], buf[1]]))
}

/// Write a big-endian u16 to the provided byte slice.
pub const fn write_u16(buf: &mut [u8; 2], val: u16) { *buf = val.to_be_bytes(); }

/// Write a big-endian u32 to the provided byte slice.
pub const fn write_u32(buf: &mut [u8; 4], val: u32) { *buf = val.to_be_bytes(); }

/// Parsed frame header according to the protocol specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameHeader {
    /// Frame flags (must be zero for protocol version 1).
    pub flags: u8,
    /// Whether this is a reply (0 = request, 1 = reply).
    pub is_reply: u8,
    /// Transaction type identifier.
    pub ty: u16,
    /// Transaction identifier for matching requests and replies.
    pub id: u32,
    /// Error code (0 indicates success).
    pub error: u32,
    /// Total size of the complete payload in bytes.
    pub total_size: u32,
    /// Size of the payload in this frame.
    pub data_size: u32,
}

impl FrameHeader {
    /// Parse a frame header from a 20-byte buffer.
    #[must_use = "use the returned header"]
    pub const fn from_bytes(buf: &[u8; HEADER_LEN]) -> Self {
        Self {
            flags: buf[0],
            is_reply: buf[1],
            ty: u16::from_be_bytes([buf[2], buf[3]]),
            id: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
            error: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
            total_size: u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]),
            data_size: u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]),
        }
    }

    /// Write the header to a 20-byte buffer.
    pub fn write_bytes(&self, buf: &mut [u8; HEADER_LEN]) {
        buf[0] = self.flags;
        buf[1] = self.is_reply;
        buf[2..4].copy_from_slice(&self.ty.to_be_bytes());
        buf[4..8].copy_from_slice(&self.id.to_be_bytes());
        buf[8..12].copy_from_slice(&self.error.to_be_bytes());
        buf[12..16].copy_from_slice(&self.total_size.to_be_bytes());
        buf[16..20].copy_from_slice(&self.data_size.to_be_bytes());
    }

    /// Parse a frame header from a byte slice.
    ///
    /// # Errors
    /// Returns an error if the slice is too short or the header fields cannot be read.
    #[must_use = "handle the result"]
    #[expect(clippy::indexing_slicing, reason = "length is checked before indexing")]
    pub fn new(buf: &[u8]) -> Result<Self, TransactionError> {
        if buf.len() < HEADER_LEN {
            return Err(TransactionError::ShortBuffer);
        }
        Ok(Self {
            flags: buf[0],
            is_reply: buf[1],
            ty: read_u16(&buf[2..4])?,
            id: read_u32(&buf[4..8])?,
            error: read_u32(&buf[8..12])?,
            total_size: read_u32(&buf[12..16])?,
            data_size: read_u32(&buf[16..20])?,
        })
    }
}

/// Complete transaction payload assembled from one or more fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    /// Transaction frame header.
    pub header: FrameHeader,
    /// Complete payload data.
    pub payload: Vec<u8>,
}

/// Parse a transaction from a single frame of bytes.
///
/// # Errors
/// Returns an error if the frame is malformed or exceeds size limits.
///
/// # Panics
/// Cannot panic; the slice conversion is guarded by an earlier length check.
#[must_use = "handle the result"]
#[expect(clippy::indexing_slicing, reason = "length is checked before slicing")]
pub fn parse_transaction(buf: &[u8]) -> Result<Transaction, TransactionError> {
    if buf.len() < HEADER_LEN {
        return Err(TransactionError::SizeMismatch);
    }
    debug_assert!(buf.len() >= HEADER_LEN, "buffer too short for header");
    #[expect(
        clippy::expect_used,
        reason = "slice conversion cannot fail after length check"
    )]
    let hdr: &[u8; HEADER_LEN] = buf[0..HEADER_LEN].try_into().expect("length checked above");
    let header = FrameHeader::from_bytes(hdr);
    if header.total_size as usize > MAX_PAYLOAD_SIZE {
        return Err(TransactionError::PayloadTooLarge);
    }
    if buf.len() != HEADER_LEN + header.total_size as usize {
        return Err(TransactionError::SizeMismatch);
    }
    let payload = buf[HEADER_LEN..].to_vec();
    let tx = Transaction { header, payload };
    validate_payload(&tx)?;
    Ok(tx)
}

impl Transaction {
    /// Serialise the transaction into a vector of bytes.
    #[must_use = "use the serialised bytes"]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_LEN + self.payload.len());
        let mut hdr = [0u8; HEADER_LEN];
        self.header.write_bytes(&mut hdr);
        buf.extend_from_slice(&hdr);
        buf.extend_from_slice(&self.payload);
        buf
    }
}

pub(super) async fn read_frame<R: AsyncRead + Unpin>(
    rdr: &mut R,
    timeout_dur: Duration,
    max_total: usize,
) -> Result<(FrameHeader, Vec<u8>), TransactionError> {
    let mut hdr_buf = [0u8; HEADER_LEN];
    read_timeout_exact(rdr, &mut hdr_buf, timeout_dur).await?;
    let hdr = FrameHeader::from_bytes(&hdr_buf);
    if hdr.total_size as usize > max_total {
        return Err(TransactionError::PayloadTooLarge);
    }
    if hdr.data_size as usize > MAX_FRAME_DATA {
        return Err(TransactionError::PayloadTooLarge);
    }
    let mut data = vec![0u8; hdr.data_size as usize];
    read_timeout_exact(rdr, &mut data, timeout_dur).await?;
    Ok((hdr, data))
}

pub(super) async fn write_frame<W: AsyncWrite + Unpin>(
    wtr: &mut W,
    mut hdr: FrameHeader,
    chunk: &[u8],
    timeout_dur: Duration,
) -> Result<(), TransactionError> {
    hdr.data_size = u32::try_from(chunk.len()).map_err(|_| TransactionError::PayloadTooLarge)?;
    let mut buf = [0u8; HEADER_LEN];
    hdr.write_bytes(&mut buf);
    write_timeout_all(wtr, &buf, timeout_dur).await?;
    write_timeout_all(wtr, chunk, timeout_dur).await
}

pub(super) async fn read_stream_chunk<R: AsyncRead + Unpin>(
    rdr: &mut R,
    buf: &mut [u8],
    timeout_dur: Duration,
) -> Result<(), TransactionError> {
    read_timeout_exact(rdr, buf, timeout_dur).await
}

pub(super) const fn default_timeout() -> Duration { IO_TIMEOUT }
