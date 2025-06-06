use std::collections::HashSet;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::field_id::FieldId;
use tokio::time::timeout;

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
/// Returns an error if `buf` is shorter than four bytes.
pub fn read_u32(buf: &[u8]) -> Result<u32, TransactionError> {
    if buf.len() < 4 {
        return Err(TransactionError::ShortBuffer);
    }
    Ok(u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]))
}

///
/// Returns an error if `buf` is shorter than two bytes.
pub fn read_u16(buf: &[u8]) -> Result<u16, TransactionError> {
    if buf.len() < 2 {
        return Err(TransactionError::ShortBuffer);
    }
    Ok(u16::from_be_bytes([buf[0], buf[1]]))
}

/// Write a big-endian u16 to the provided byte slice.
pub fn write_u16(buf: &mut [u8], val: u16) {
    buf.copy_from_slice(&val.to_be_bytes());
}

/// Write a big-endian u32 to the provided byte slice.
pub fn write_u32(buf: &mut [u8], val: u32) {
    buf.copy_from_slice(&val.to_be_bytes());
}

/// Parsed frame header according to the protocol specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameHeader {
    pub flags: u8,
    pub is_reply: u8,
    pub ty: u16,
    pub id: u32,
    pub error: u32,
    pub total_size: u32,
    pub data_size: u32,
}

impl FrameHeader {
    /// Parse a frame header from a 20-byte buffer.
    pub fn from_bytes(buf: &[u8; HEADER_LEN]) -> Self {
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
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

/// Parse a transaction from a single frame of bytes.
#[cfg_attr(test, allow(dead_code))]
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
    #[cfg_attr(test, allow(dead_code))]
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
    hdr.data_size = chunk.len() as u32;
    let mut buf = [0u8; HEADER_LEN];
    hdr.write_bytes(&mut buf);
    write_timeout_all(wtr, &buf, timeout_dur).await?;
    write_timeout_all(wtr, chunk, timeout_dur).await
}

/// Errors that can occur when parsing or writing transactions.
#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("invalid flags")] // flags must be zero for v1.8.5
    InvalidFlags,
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("size mismatch")]
    SizeMismatch,
    #[error("duplicate field id {0}")]
    DuplicateField(u16),
    #[error("buffer too short")]
    ShortBuffer,
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("I/O timeout")]
    Timeout,
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
        if !seen.insert(field_id) {
            return Err(TransactionError::DuplicateField(field_id));
        }
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
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            timeout: IO_TIMEOUT,
            max_payload: MAX_PAYLOAD_SIZE,
        }
    }

    /// Read the next complete transaction from the underlying reader.
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
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            timeout: IO_TIMEOUT,
            max_frame: MAX_FRAME_DATA,
            max_payload: MAX_PAYLOAD_SIZE,
        }
    }

    /// Write a transaction to the stream, fragmenting if necessary.
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
#[cfg_attr(test, allow(dead_code))]
pub fn decode_params(buf: &[u8]) -> Result<Vec<(FieldId, Vec<u8>)>, TransactionError> {
    if buf.is_empty() {
        return Ok(Vec::new());
    }
    if buf.len() < 2 {
        return Err(TransactionError::SizeMismatch);
    }
    let count = read_u16(&buf[0..2])? as usize;
    let mut offset = 2usize;
    let mut params = Vec::with_capacity(count);
    let mut seen = HashSet::new();
    for _ in 0..count {
        if offset + 4 > buf.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let field_id = read_u16(&buf[offset..offset + 2])?;
        let field_size = read_u16(&buf[offset + 2..offset + 4])? as usize;
        offset += 4;
        if offset + field_size > buf.len() {
            return Err(TransactionError::SizeMismatch);
        }
        if !seen.insert(field_id) {
            return Err(TransactionError::DuplicateField(field_id));
        }
        params.push((
            FieldId::from(field_id),
            buf[offset..offset + field_size].to_vec(),
        ));
        offset += field_size;
    }
    if offset != buf.len() {
        return Err(TransactionError::SizeMismatch);
    }
    Ok(params)
}

/// Build a parameter block from field id/data pairs.
#[cfg_attr(test, allow(dead_code))]
pub fn encode_params(params: &[(FieldId, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(params.len() as u16).to_be_bytes());
    for (id, data) in params {
        let raw: u16 = (*id).into();
        buf.extend_from_slice(&raw.to_be_bytes());
        buf.extend_from_slice(&(data.len() as u16).to_be_bytes());
        buf.extend_from_slice(data);
    }
    buf
}
