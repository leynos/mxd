//! Buffered and streaming writers for Hotline transactions.
//!
//! [`TransactionWriter`] serialises and fragments a complete [`Transaction`].
//! For very large payloads, use [`TransactionWriter::write_streaming`] to
//! stream bytes from an [`AsyncRead`] source without buffering the full
//! payload.

use std::time::Duration;

use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    time::timeout,
};

use super::{
    FrameHeader,
    IO_TIMEOUT,
    MAX_FRAME_DATA,
    MAX_PAYLOAD_SIZE,
    Transaction,
    errors::TransactionError,
    frame::{read_stream_chunk, write_frame},
    params::validate_payload,
};

/// Translate early EOF from a source stream into a semantic framing error.
fn map_eof_to_size_mismatch(err: TransactionError) -> TransactionError {
    if matches!(
        &err,
        TransactionError::Io(io) if io.kind() == std::io::ErrorKind::UnexpectedEof
    ) {
        TransactionError::SizeMismatch
    } else {
        err
    }
}

/// Buffered writer for Hotline transactions.
///
/// Fragments large payloads across multiple frames according to the configured
/// maximum frame size. Use [`write_transaction`](Self::write_transaction) for
/// fully buffered payloads or [`write_streaming`](Self::write_streaming) to
/// stream from an [`AsyncRead`] source without buffering.
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
    async fn flush_timeout(&mut self) -> Result<(), TransactionError> {
        timeout(self.timeout, self.writer.flush())
            .await
            .map_err(|_| TransactionError::Timeout)??;
        Ok(())
    }

    /// Create a new writer with default timeout and size limits.
    #[must_use = "create a writer"]
    #[expect(
        clippy::missing_const_for_fn,
        reason = "const fn with non-const trait bounds (AsyncWrite + Unpin) is misleading"
    )]
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            timeout: IO_TIMEOUT,
            max_frame: MAX_FRAME_DATA,
            max_payload: MAX_PAYLOAD_SIZE,
        }
    }

    /// Override the I/O timeout used for writes.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the maximum frame size used for fragmentation.
    ///
    /// The value is clamped to `1..=MAX_FRAME_DATA` to prevent infinite loops
    /// (if zero) and to ensure frames are accepted by readers.
    #[must_use]
    pub const fn with_max_frame(mut self, max_frame: usize) -> Self {
        self.max_frame = if max_frame == 0 {
            1
        } else if max_frame > MAX_FRAME_DATA {
            MAX_FRAME_DATA
        } else {
            max_frame
        };
        self
    }

    /// Override the maximum payload size accepted for buffered writes.
    #[must_use]
    pub const fn with_max_payload(mut self, max_payload: usize) -> Self {
        self.max_payload = max_payload;
        self
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
        if tx.payload.is_empty() {
            write_frame(&mut self.writer, tx.header.clone(), &[], self.timeout).await?;
            self.flush_timeout().await?;
            return Ok(());
        }

        let mut offset = 0usize;
        while offset < tx.payload.len() {
            let end = (offset + self.max_frame).min(tx.payload.len());
            let Some(chunk) = tx.payload.get(offset..end) else {
                return Err(TransactionError::SizeMismatch);
            };
            write_frame(&mut self.writer, tx.header.clone(), chunk, self.timeout).await?;
            offset = end;
        }
        self.flush_timeout().await?;
        Ok(())
    }

    /// Write a payload from a reader, emitting fragments incrementally.
    ///
    /// The caller must set `header.total_size` to the total byte count that
    /// will be read from `source`. The payload is not validated as a parameter
    /// list; this method is intended for raw file transfers.
    ///
    /// # Errors
    /// Returns an error if the header is invalid, the stream ends early, or an
    /// I/O error occurs while writing fragments.
    pub async fn write_streaming<R>(
        &mut self,
        header: FrameHeader,
        mut source: R,
    ) -> Result<(), TransactionError>
    where
        R: AsyncRead + Unpin,
    {
        if header.flags != 0 {
            return Err(TransactionError::InvalidFlags);
        }
        let total = header.total_size;
        if total as usize > self.max_payload {
            return Err(TransactionError::PayloadTooLarge);
        }

        if total == 0 {
            write_frame(&mut self.writer, header, &[], self.timeout).await?;
            self.flush_timeout().await?;
            return Ok(());
        }

        let mut buf = vec![0u8; self.max_frame];
        let mut sent = 0u32;
        while sent < total {
            let remaining = (total - sent) as usize;
            let to_read = remaining.min(self.max_frame);
            let chunk = buf
                .get_mut(..to_read)
                .ok_or(TransactionError::PayloadTooLarge)?;
            read_stream_chunk(&mut source, chunk, self.timeout)
                .await
                .map_err(map_eof_to_size_mismatch)?;
            write_frame(&mut self.writer, header.clone(), chunk, self.timeout).await?;
            sent += u32::try_from(to_read).map_err(|_| TransactionError::PayloadTooLarge)?;
        }
        self.flush_timeout().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tokio::io::{BufReader, BufWriter, duplex};

    use super::*;
    use crate::transaction::reader::TransactionStreamReader;

    #[expect(
        clippy::excessive_nesting,
        reason = "spawned task requires extra nesting"
    )]
    #[tokio::test]
    async fn write_streaming_fragments_payload() {
        let payload = vec![1u8; 100_000];
        let (client, server) = duplex(16 * 1024);
        let mut writer =
            TransactionWriter::new(BufWriter::new(server)).with_max_payload(payload.len() + 1);
        let header = FrameHeader {
            flags: 0,
            is_reply: 1,
            ty: 202,
            id: 9,
            error: 0,
            total_size: u32::try_from(payload.len()).expect("len fits"),
            data_size: 0,
        };

        let header_for_reader = header.clone();
        let payload_len = payload.len();
        let reader_task = tokio::spawn(async move {
            let mut stream_reader = TransactionStreamReader::new(BufReader::new(client))
                .with_max_total(payload_len + 1);
            let mut stream = stream_reader.start_transaction().await.expect("stream");

            let mut seen = Vec::new();
            while let Some(fragment) = stream.next_fragment().await.expect("fragment") {
                seen.extend_from_slice(&fragment.payload);
            }

            (
                seen,
                stream.header().ty,
                stream.header().id,
                header_for_reader,
            )
        });

        writer
            .write_streaming(header.clone(), BufReader::new(Cursor::new(payload.clone())))
            .await
            .expect("write streaming");

        let (seen, ty, id, expected_header) = reader_task.await.expect("reader task");
        assert_eq!(seen, payload);
        assert_eq!(ty, expected_header.ty);
        assert_eq!(id, expected_header.id);
    }

    /// Verifies that `write_streaming` returns `SizeMismatch` when the source
    /// stream ends before supplying the promised `total_size` bytes.
    #[tokio::test]
    async fn write_streaming_truncated_source_returns_size_mismatch() {
        let actual_bytes = vec![1u8; 50];
        let promised_total: u32 = 100_000;

        let (_, server) = duplex(16 * 1024);
        let mut writer = TransactionWriter::new(BufWriter::new(server))
            .with_max_payload(promised_total as usize + 1);

        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 100,
            id: 1,
            error: 0,
            total_size: promised_total,
            data_size: 0,
        };

        let result = writer
            .write_streaming(header, Cursor::new(actual_bytes))
            .await;

        assert!(
            matches!(result, Err(TransactionError::SizeMismatch)),
            "expected SizeMismatch, got {result:?}"
        );
    }
}
