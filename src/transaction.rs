use std::collections::HashSet;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

/// Length of a transaction frame header in bytes.
pub const HEADER_LEN: usize = 20;
/// Maximum allowed payload size for a complete transaction.
pub const MAX_PAYLOAD_SIZE: usize = 1024 * 1024; // 1 MiB
/// Maximum data size per frame when writing.
pub const MAX_FRAME_DATA: usize = 32 * 1024; // 32 KiB
/// Default I/O timeout when reading or writing transactions.
pub const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Read a big-endian u16 from the provided byte slice.
pub fn read_u16(buf: &[u8]) -> u16 {
    u16::from_be_bytes([buf[0], buf[1]])
}

/// Read a big-endian u32 from the provided byte slice.
pub fn read_u32(buf: &[u8]) -> u32 {
    u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
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
            ty: read_u16(&buf[2..4]),
            id: read_u32(&buf[4..8]),
            error: read_u32(&buf[8..12]),
            total_size: read_u32(&buf[12..16]),
            data_size: read_u32(&buf[16..20]),
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
}

/// Complete transaction payload assembled from one or more fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
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
    let param_count = read_u16(&tx.payload[0..2]) as usize;
    let mut offset = 2;
    let mut seen = HashSet::new();
    for _ in 0..param_count {
        if offset + 4 > tx.payload.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let field_id = read_u16(&tx.payload[offset..offset + 2]);
        let field_size = read_u16(&tx.payload[offset + 2..offset + 4]) as usize;
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
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            timeout: IO_TIMEOUT,
            max_payload: MAX_PAYLOAD_SIZE,
        }
    }

    /// Read the next complete transaction from the underlying reader.
    pub async fn read_transaction(&mut self) -> Result<Transaction, TransactionError> {
        let mut header_buf = [0u8; HEADER_LEN];
        timeout(self.timeout, self.reader.read_exact(&mut header_buf))
            .await
            .map_err(|_| TransactionError::Timeout)??;
        let mut header = FrameHeader::from_bytes(&header_buf);
        if header.flags != 0 {
            return Err(TransactionError::InvalidFlags);
        }
        if header.total_size as usize > self.max_payload {
            return Err(TransactionError::PayloadTooLarge);
        }
        if header.data_size > header.total_size {
            return Err(TransactionError::SizeMismatch);
        }
        let mut payload = vec![0u8; header.data_size as usize];
        timeout(self.timeout, self.reader.read_exact(&mut payload))
            .await
            .map_err(|_| TransactionError::Timeout)??;
        let mut remaining = header.total_size - header.data_size;
        while remaining > 0 {
            timeout(self.timeout, self.reader.read_exact(&mut header_buf))
                .await
                .map_err(|_| TransactionError::Timeout)??;
            let next = FrameHeader::from_bytes(&header_buf);
            if next.flags != header.flags
                || next.is_reply != header.is_reply
                || next.ty != header.ty
                || next.id != header.id
                || next.error != header.error
                || next.total_size != header.total_size
            {
                return Err(TransactionError::SizeMismatch);
            }
            if next.data_size > remaining {
                return Err(TransactionError::SizeMismatch);
            }
            let mut buf = vec![0u8; next.data_size as usize];
            timeout(self.timeout, self.reader.read_exact(&mut buf))
                .await
                .map_err(|_| TransactionError::Timeout)??;
            payload.extend_from_slice(&buf);
            remaining -= next.data_size;
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
        let mut header = tx.header.clone();
        while offset < tx.payload.len() {
            let remaining = tx.payload.len() - offset;
            let chunk = remaining.min(self.max_frame);
            header.data_size = chunk as u32;
            let mut buf = [0u8; HEADER_LEN];
            header.write_bytes(&mut buf);
            timeout(self.timeout, self.writer.write_all(&buf))
                .await
                .map_err(|_| TransactionError::Timeout)??;
            timeout(
                self.timeout,
                self.writer.write_all(&tx.payload[offset..offset + chunk]),
            )
            .await
            .map_err(|_| TransactionError::Timeout)??;
            offset += chunk;
        }
        timeout(self.timeout, self.writer.flush())
            .await
            .map_err(|_| TransactionError::Timeout)??;
        Ok(())
    }
}
