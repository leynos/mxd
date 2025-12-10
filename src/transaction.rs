//! Provides asynchronous helpers for framing, encoding, and decoding transactions.
//!
//! Transactions consist of a [`FrameHeader`] followed by an optional payload
//! encoded using [`FieldId`] identifiers.

#![allow(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
#![allow(
    clippy::indexing_slicing,
    reason = "array bounds are validated earlier in parsing"
)]
#![allow(clippy::unwrap_used, reason = "slice conversions are length-validated")]

use std::{collections::HashSet, time::Duration};

use thiserror::Error;
use tokio::{
    io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::timeout,
};

use crate::field_id::FieldId;

/// Length of a transaction frame header in bytes.
pub const HEADER_LEN: usize = 20;
/// Maximum allowed payload size for a complete transaction.
pub const MAX_PAYLOAD_SIZE: usize = 1024 * 1024; // 1 MiB
/// Maximum data size per frame when writing.
pub const MAX_FRAME_DATA: usize = 32 * 1024; // 32 KiB
/// Default I/O timeout when reading or writing transactions.
pub const IO_TIMEOUT: Duration = Duration::from_secs(5);

async fn read_timeout_exact<R: AsyncRead + Unpin>(
    r: &mut R,
    buf: &mut [u8],
    timeout_dur: Duration,
) -> Result<(), TransactionError> {
    timeout(timeout_dur, r.read_exact(buf))
        .await
        .map_err(|_| TransactionError::Timeout)??;
    Ok(())
}

async fn write_timeout_all<W: AsyncWrite + Unpin>(
    w: &mut W,
    buf: &[u8],
    timeout_dur: Duration,
) -> Result<(), TransactionError> {
    timeout(timeout_dur, w.write_all(buf))
        .await
        .map_err(|_| TransactionError::Timeout)??;
    Ok(())
}

/// Read a big-endian `u32` from the provided byte slice.
///
/// # Errors
/// Returns an error if `buf` is shorter than four bytes.
#[must_use = "handle the result"]
pub fn read_u32(buf: &[u8]) -> Result<u32, TransactionError> {
    if buf.len() < 4 {
        return Err(TransactionError::ShortBuffer);
    }
    Ok(u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]))
}

///
/// # Errors
/// Returns an error if `buf` is shorter than two bytes.
#[must_use = "handle the result"]
pub fn read_u16(buf: &[u8]) -> Result<u16, TransactionError> {
    if buf.len() < 2 {
        return Err(TransactionError::ShortBuffer);
    }
    Ok(u16::from_be_bytes([buf[0], buf[1]]))
}

/// Write a big-endian u16 to the provided byte slice.
#[expect(clippy::missing_const_for_fn, reason = "copy_from_slice is not const")]
pub fn write_u16(buf: &mut [u8], val: u16) { buf.copy_from_slice(&val.to_be_bytes()); }

/// Write a big-endian u32 to the provided byte slice.
#[expect(clippy::missing_const_for_fn, reason = "copy_from_slice is not const")]
pub fn write_u32(buf: &mut [u8], val: u32) { buf.copy_from_slice(&val.to_be_bytes()); }

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
        write_u16(&mut buf[2..4], self.ty);
        write_u32(&mut buf[4..8], self.id);
        write_u32(&mut buf[8..12], self.error);
        write_u32(&mut buf[12..16], self.total_size);
        write_u32(&mut buf[16..20], self.data_size);
    }

    /// Parse a frame header from a byte slice.
    ///
    /// # Errors
    /// Returns an error if the slice is too short or the header fields cannot be read.
    #[must_use = "handle the result"]
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
/// # Panics
/// Panics if `buf` is shorter than [`HEADER_LEN`], which is checked earlier.
///
/// # Errors
/// Returns an error if the frame is malformed or exceeds size limits.
#[cfg_attr(test, expect(dead_code, reason = "used in integration tests"))]
#[must_use = "handle the result"]
pub fn parse_transaction(buf: &[u8]) -> Result<Transaction, TransactionError> {
    if buf.len() < HEADER_LEN {
        return Err(TransactionError::SizeMismatch);
    }
    let hdr: &[u8; HEADER_LEN] = buf[0..HEADER_LEN].try_into().unwrap();
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
    /// Serialize the transaction into a vector of bytes.
    #[cfg_attr(test, expect(dead_code, reason = "used in integration tests"))]
    #[must_use = "use the serialized bytes"]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_LEN + self.payload.len());
        let mut hdr = [0u8; HEADER_LEN];
        self.header.write_bytes(&mut hdr);
        buf.extend_from_slice(&hdr);
        buf.extend_from_slice(&self.payload);
        buf
    }
}

async fn read_frame<R: AsyncRead + Unpin>(
    rdr: &mut R,
    timeout_dur: Duration,
) -> Result<(FrameHeader, Vec<u8>), TransactionError> {
    let mut hdr_buf = [0u8; HEADER_LEN];
    read_timeout_exact(rdr, &mut hdr_buf, timeout_dur).await?;
    let hdr = FrameHeader::from_bytes(&hdr_buf);
    if hdr.total_size as usize > MAX_PAYLOAD_SIZE || hdr.data_size as usize > MAX_PAYLOAD_SIZE {
        return Err(TransactionError::PayloadTooLarge);
    }
    let mut data = vec![0u8; hdr.data_size as usize];
    read_timeout_exact(rdr, &mut data, timeout_dur).await?;
    Ok((hdr, data))
}

async fn write_frame<W: AsyncWrite + Unpin>(
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

/// Determine whether duplicate instances of the given field id are permitted.
const fn duplicate_allowed(fid: FieldId) -> bool {
    matches!(
        fid,
        FieldId::NewsCategory | FieldId::NewsArticle | FieldId::FileName
    )
}

fn check_duplicate(fid: FieldId, seen: &mut HashSet<u16>) -> Result<(), TransactionError> {
    let raw: u16 = fid.into();
    if !duplicate_allowed(fid) && !seen.insert(raw) {
        return Err(TransactionError::DuplicateField(raw));
    }
    Ok(())
}

/// Validate the assembled transaction payload for duplicate fields and length
/// correctness according to the protocol specification.
fn validate_payload(tx: &Transaction) -> Result<(), TransactionError> {
    if tx.header.total_size as usize != tx.payload.len() {
        return Err(TransactionError::SizeMismatch);
    }
    if tx.payload.is_empty() {
        return Ok(());
    }
    if tx.payload.len() < 2 {
        return Err(TransactionError::SizeMismatch);
    }
    let param_count = read_u16(&tx.payload[0..2])? as usize;
    let mut offset = 2;
    let mut seen = HashSet::new();
    for _ in 0..param_count {
        if offset + 4 > tx.payload.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let field_id = read_u16(&tx.payload[offset..offset + 2])?;
        let field_size = read_u16(&tx.payload[offset + 2..offset + 4])? as usize;
        offset += 4;
        if offset + field_size > tx.payload.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let fid = FieldId::from(field_id);
        check_duplicate(fid, &mut seen)?;
        offset += field_size;
    }
    if offset != tx.payload.len() {
        return Err(TransactionError::SizeMismatch);
    }
    Ok(())
}

/// Reader for assembling complete transactions from a byte stream.
pub struct TransactionReader<R> {
    reader: R,
    timeout: Duration,
    max_payload: usize,
}

impl<R> TransactionReader<R>
where
    R: AsyncRead + Unpin,
{
    /// Create a new reader with default timeout and payload limits.
    #[must_use = "create a reader"]
    pub const fn new(reader: R) -> Self {
        Self {
            reader,
            timeout: IO_TIMEOUT,
            max_payload: MAX_PAYLOAD_SIZE,
        }
    }

    /// Read the next complete transaction from the underlying reader.
    ///
    /// # Errors
    /// Returns an error if the stream does not contain a valid transaction.
    #[must_use = "handle the result"]
    pub async fn read_transaction(&mut self) -> Result<Transaction, TransactionError> {
        let (first_hdr, mut payload) = read_frame(&mut self.reader, self.timeout).await?;
        let mut header = first_hdr.clone();
        if header.flags != 0 {
            return Err(TransactionError::InvalidFlags);
        }
        if header.total_size as usize > self.max_payload {
            return Err(TransactionError::PayloadTooLarge);
        }
        if header.data_size > header.total_size {
            return Err(TransactionError::SizeMismatch);
        }
        let mut remaining = header.total_size - header.data_size;
        while remaining > 0 {
            let (next_hdr, chunk) = read_frame(&mut self.reader, self.timeout).await?;
            if next_hdr.ty != header.ty
                || next_hdr.id != header.id
                || next_hdr.error != header.error
                || next_hdr.total_size != header.total_size
                || next_hdr.flags != header.flags
                || next_hdr.is_reply != header.is_reply
            {
                return Err(TransactionError::SizeMismatch);
            }
            if next_hdr.data_size > remaining {
                return Err(TransactionError::SizeMismatch);
            }
            payload.extend_from_slice(&chunk);
            remaining -= next_hdr.data_size;
        }
        header.data_size = header.total_size;
        let tx = Transaction { header, payload };
        validate_payload(&tx)?;
        Ok(tx)
    }
}

/// Writer for sending transactions over a byte stream.
pub struct TransactionWriter<W> {
    writer: W,
    timeout: Duration,
    max_frame: usize,
    max_payload: usize,
}

impl<W> TransactionWriter<W>
where
    W: AsyncWrite + Unpin,
{
    /// Create a new writer with default timeout and size limits.
    #[must_use = "create a writer"]
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            timeout: IO_TIMEOUT,
            max_frame: MAX_FRAME_DATA,
            max_payload: MAX_PAYLOAD_SIZE,
        }
    }

    /// Write a transaction to the stream, fragmenting if necessary.
    ///
    /// # Errors
    /// Returns an error if writing to the stream fails or the transaction is invalid.
    #[must_use = "handle the result"]
    pub async fn write_transaction(&mut self, tx: &Transaction) -> Result<(), TransactionError> {
        if tx.header.flags != 0 {
            return Err(TransactionError::InvalidFlags);
        }
        validate_payload(tx)?;
        if tx.header.total_size as usize > self.max_payload {
            return Err(TransactionError::PayloadTooLarge);
        }
        let mut offset = 0usize;
        let header = tx.header.clone();
        if tx.payload.is_empty() {
            write_frame(&mut self.writer, header, &[], self.timeout).await?;
        } else {
            while offset < tx.payload.len() {
                let end = (offset + self.max_frame).min(tx.payload.len());
                write_frame(
                    &mut self.writer,
                    header.clone(),
                    &tx.payload[offset..end],
                    self.timeout,
                )
                .await?;
                offset = end;
            }
        }
        timeout(self.timeout, self.writer.flush())
            .await
            .map_err(|_| TransactionError::Timeout)??;
        Ok(())
    }
}

/// Decode the parameter block of a transaction into a vector of field id/data pairs.
///
/// # Errors
/// Returns an error if the buffer is malformed or shorter than expected.
#[cfg_attr(test, expect(dead_code, reason = "used in integration tests"))]
#[must_use = "handle the result"]
pub fn decode_params(buf: &[u8]) -> Result<Vec<(FieldId, Vec<u8>)>, TransactionError> {
    if buf.is_empty() {
        return Ok(Vec::new());
    }
    if buf.len() < 2 {
        return Err(TransactionError::SizeMismatch);
    }
    // read the parameter count; treat a short buffer as a size mismatch
    let count = read_u16(&buf[0..2]).map_err(|_| TransactionError::SizeMismatch)? as usize;
    let mut offset = 2usize;
    let mut params = Vec::with_capacity(count);
    let mut seen = HashSet::new();
    for _ in 0..count {
        if offset + 4 > buf.len() {
            return Err(TransactionError::SizeMismatch);
        }
        // errors here indicate the buffer length did not match the stated size,
        // so map them to `SizeMismatch`
        let field_id =
            read_u16(&buf[offset..offset + 2]).map_err(|_| TransactionError::SizeMismatch)?;
        let field_size = read_u16(&buf[offset + 2..offset + 4])
            .map_err(|_| TransactionError::SizeMismatch)? as usize;
        offset += 4;
        if offset + field_size > buf.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let fid = FieldId::from(field_id);
        check_duplicate(fid, &mut seen)?;
        params.push((fid, buf[offset..offset + field_size].to_vec()));
        offset += field_size;
    }
    if offset != buf.len() {
        return Err(TransactionError::SizeMismatch);
    }
    Ok(params)
}

/// Decode the parameter block into a map keyed by `FieldId`.
///
/// # Errors
/// Returns an error if the buffer cannot be parsed.
#[must_use = "handle the result"]
pub fn decode_params_map(
    buf: &[u8],
) -> Result<std::collections::HashMap<FieldId, Vec<Vec<u8>>>, TransactionError> {
    let params = decode_params(buf)?;
    let mut map: std::collections::HashMap<FieldId, Vec<Vec<u8>>> =
        std::collections::HashMap::new();
    for (fid, value) in params {
        map.entry(fid).or_default().push(value);
    }
    Ok(map)
}

/// Build a parameter block from field id/data pairs.
///
/// # Errors
/// Returns [`TransactionError::PayloadTooLarge`] if the number of parameters
/// or any data length exceeds `u16::MAX`.
#[must_use = "use the encoded bytes"]
pub fn encode_params(params: &[(FieldId, &[u8])]) -> Result<Vec<u8>, TransactionError> {
    let mut buf = Vec::new();
    buf.extend_from_slice(
        &u16::try_from(params.len())
            .map_err(|_| TransactionError::PayloadTooLarge)?
            .to_be_bytes(),
    );
    for (id, data) in params {
        let raw: u16 = (*id).into();
        buf.extend_from_slice(&raw.to_be_bytes());
        buf.extend_from_slice(
            &u16::try_from(data.len())
                .map_err(|_| TransactionError::PayloadTooLarge)?
                .to_be_bytes(),
        );
        buf.extend_from_slice(data);
    }
    Ok(buf)
}

/// Convenience for encoding a vector of owned parameter values.
///
/// This converts a `&[(FieldId, Vec<u8>)]` slice into the borrowed
/// form expected by [`encode_params`]. It avoids repeating the
/// conversion logic at call sites.
///
/// # Errors
/// Returns [`TransactionError`] if the inner call to [`encode_params`]
/// fails, for example when the payload is too large.
#[must_use = "use the encoded bytes"]
pub fn encode_vec_params(params: &[(FieldId, Vec<u8>)]) -> Result<Vec<u8>, TransactionError> {
    let borrowed: Vec<(FieldId, &[u8])> = params
        .iter()
        .map(|(id, bytes)| (*id, bytes.as_slice()))
        .collect();
    encode_params(&borrowed)
}

/// Return the first value for `field` in a parameter map as a `String`.
///
/// Returns `Ok(None)` if the field is absent and an error if the bytes are not
/// valid UTF-8.
///
/// # Errors
/// Returns an error if the parameter value is not valid UTF-8.
#[must_use = "handle the result"]
pub fn first_param_string<S: std::hash::BuildHasher>(
    map: &std::collections::HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Result<Option<String>, &'static str> {
    match map.get(&field).and_then(|v| v.first()) {
        Some(bytes) => Ok(Some(String::from_utf8(bytes.clone()).map_err(|_| "utf8")?)),
        None => Ok(None),
    }
}

/// Return the first value for `field` as a `String` or an error if missing.
///
/// # Errors
/// Returns an error if the field is missing or not valid UTF-8.
#[must_use = "handle the result"]
pub fn required_param_string<S: std::hash::BuildHasher>(
    map: &std::collections::HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
    missing_err: &'static str,
) -> Result<String, &'static str> {
    first_param_string(map, field)?.ok_or(missing_err)
}

/// Decode the first value for `field` as a big-endian `i32`.
///
/// # Errors
/// Returns an error if the field is missing or cannot be parsed as `i32`.
#[must_use = "handle the result"]
pub fn required_param_i32<S: std::hash::BuildHasher>(
    map: &std::collections::HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
    missing_err: &'static str,
    parse_err: &'static str,
) -> Result<i32, &'static str> {
    let bytes = map.get(&field).and_then(|v| v.first()).ok_or(missing_err)?;
    let arr: [u8; 4] = bytes.as_slice().try_into().map_err(|_| parse_err)?;
    Ok(i32::from_be_bytes(arr))
}

/// Decode the first value for `field` as an `i32` if present.
///
/// Returns `Ok(None)` if the parameter is absent and an error if it is present
/// but does not decode as a big-endian `i32`.
///
/// # Errors
/// Returns an error if the value cannot be parsed as `i32`.
#[must_use = "handle the result"]
pub fn first_param_i32<S: std::hash::BuildHasher>(
    map: &std::collections::HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
    parse_err: &'static str,
) -> Result<Option<i32>, &'static str> {
    match map.get(&field).and_then(|v| v.first()) {
        Some(bytes) => {
            let arr: [u8; 4] = bytes.as_slice().try_into().map_err(|_| parse_err)?;
            Ok(Some(i32::from_be_bytes(arr)))
        }
        None => Ok(None),
    }
}
