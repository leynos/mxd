//! Buffered and streaming readers for Hotline transactions.
//!
//! [`TransactionReader`] assembles entire payloads into memory for small
//! request/response frames. [`TransactionStreamReader`] exposes an incremental
//! interface that yields payload fragments without buffering the full message,
//! enabling large file transfers to be processed safely.

use std::time::Duration;

use tokio::io::AsyncRead;

use super::{
    FrameHeader,
    IO_TIMEOUT,
    MAX_PAYLOAD_SIZE,
    Transaction,
    errors::TransactionError,
    frame::{default_timeout, read_frame},
    params::validate_payload,
};

const fn headers_match(first: &FrameHeader, next: &FrameHeader) -> bool {
    next.ty == first.ty
        && next.id == first.id
        && next.error == first.error
        && next.total_size == first.total_size
        && next.flags == first.flags
        && next.is_reply == first.is_reply
}

/// Validate and read the first frame of a streaming transaction.
async fn validate_first_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    timeout: Duration,
    max_limit: usize,
) -> Result<(FrameHeader, Vec<u8>, u32), TransactionError> {
    let (first_hdr, first_chunk) = read_frame(reader, timeout, max_limit).await?;

    if first_hdr.flags != 0 {
        return Err(TransactionError::InvalidFlags);
    }
    if first_hdr.total_size as usize > max_limit {
        return Err(TransactionError::PayloadTooLarge);
    }
    if first_hdr.data_size > first_hdr.total_size {
        return Err(TransactionError::SizeMismatch);
    }
    if first_hdr.data_size == 0 && first_hdr.total_size > 0 {
        return Err(TransactionError::SizeMismatch);
    }

    let remaining = first_hdr.total_size - first_hdr.data_size;
    Ok((first_hdr, first_chunk, remaining))
}

/// Reader for assembling complete transactions from a byte stream.
pub struct TransactionReader<R> {
    reader: R,
    timeout: Duration,
    max_payload: usize,
}

/// Configuration for frame reading operations.
struct FrameReadConfig {
    timeout: Duration,
    max_payload: usize,
}

/// Accumulator for assembling a multi-frame transaction payload.
struct FrameAccumulator<'a> {
    header: &'a FrameHeader,
    payload: &'a mut Vec<u8>,
    remaining: u32,
}

const fn validate_first_header(
    header: &FrameHeader,
    max_payload: usize,
) -> Result<(), TransactionError> {
    if header.flags != 0 {
        return Err(TransactionError::InvalidFlags);
    }
    if header.total_size as usize > max_payload {
        return Err(TransactionError::PayloadTooLarge);
    }
    if header.data_size > header.total_size {
        return Err(TransactionError::SizeMismatch);
    }
    if header.data_size == 0 && header.total_size > 0 {
        return Err(TransactionError::SizeMismatch);
    }
    Ok(())
}

async fn read_continuation_frames<R: AsyncRead + Unpin>(
    reader: &mut R,
    accumulator: &mut FrameAccumulator<'_>,
    config: &FrameReadConfig,
) -> Result<(), TransactionError> {
    while accumulator.remaining > 0 {
        let (next_hdr, chunk) = read_frame(reader, config.timeout, config.max_payload).await?;
        if !headers_match(accumulator.header, &next_hdr) {
            return Err(TransactionError::SizeMismatch);
        }
        if next_hdr.data_size == 0 || next_hdr.data_size > accumulator.remaining {
            return Err(TransactionError::SizeMismatch);
        }
        accumulator.payload.extend_from_slice(&chunk);
        accumulator.remaining -= next_hdr.data_size;
    }
    Ok(())
}

impl<R> TransactionReader<R>
where
    R: AsyncRead + Unpin,
{
    /// Create a new reader with default timeout and payload limits.
    #[must_use = "create a reader"]
    #[expect(
        clippy::missing_const_for_fn,
        reason = "const fn with non-const trait bounds (AsyncRead + Unpin) is misleading"
    )]
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            timeout: IO_TIMEOUT,
            max_payload: MAX_PAYLOAD_SIZE,
        }
    }

    /// Override the I/O timeout used for reads.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the maximum buffered payload size.
    #[must_use]
    pub const fn with_max_payload(mut self, max_payload: usize) -> Self {
        self.max_payload = max_payload;
        self
    }

    /// Read the next complete transaction from the underlying reader.
    ///
    /// # Errors
    /// Returns an error if the stream does not contain a valid transaction.
    #[must_use = "handle the result"]
    pub async fn read_transaction(&mut self) -> Result<Transaction, TransactionError> {
        let (first_hdr, mut payload) =
            read_frame(&mut self.reader, self.timeout, self.max_payload).await?;
        let mut header = first_hdr.clone();
        validate_first_header(&header, self.max_payload)?;

        let remaining = header.total_size - header.data_size;
        let config = FrameReadConfig {
            timeout: self.timeout,
            max_payload: self.max_payload,
        };
        let mut accumulator = FrameAccumulator {
            header: &header,
            payload: &mut payload,
            remaining,
        };
        read_continuation_frames(&mut self.reader, &mut accumulator, &config).await?;

        header.data_size = header.total_size;
        let tx = Transaction { header, payload };
        validate_payload(&tx)?;
        Ok(tx)
    }

    /// Read the next transaction as a streaming fragment iterator.
    ///
    /// # Errors
    /// Returns an error if the first frame is invalid.
    pub async fn read_streaming_transaction(
        &mut self,
    ) -> Result<StreamingTransaction<'_, R>, TransactionError> {
        let (first_hdr, first_chunk, remaining) =
            validate_first_frame(&mut self.reader, self.timeout, self.max_payload).await?;
        Ok(StreamingTransaction {
            reader: &mut self.reader,
            first_header: first_hdr,
            timeout: self.timeout,
            remaining,
            offset: 0,
            pending_first: Some(first_chunk),
            max_total: self.max_payload,
        })
    }
}

/// A single payload fragment yielded by [`StreamingTransaction`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionFragment {
    /// Header for the fragment.
    pub header: FrameHeader,
    /// Payload bytes contained in this fragment.
    pub payload: Vec<u8>,
    /// Offset into the logical payload before this fragment.
    pub offset: u32,
    /// Whether this fragment completes the payload.
    pub is_last: bool,
}

/// Streaming view over a multi-fragment transaction.
pub struct StreamingTransaction<'a, R> {
    reader: &'a mut R,
    first_header: FrameHeader,
    timeout: Duration,
    remaining: u32,
    offset: u32,
    pending_first: Option<Vec<u8>>,
    max_total: usize,
}

impl<R> StreamingTransaction<'_, R>
where
    R: AsyncRead + Unpin,
{
    /// Return the base header (from the first fragment).
    #[must_use]
    pub const fn header(&self) -> &FrameHeader { &self.first_header }

    /// Yield the next fragment in the sequence.
    ///
    /// Returns `Ok(None)` once the final fragment has been returned.
    ///
    /// # Errors
    /// Returns an error if framing invariants are violated.
    pub async fn next_fragment(&mut self) -> Result<Option<TransactionFragment>, TransactionError> {
        if let Some(first) = self.pending_first.take() {
            let is_last = self.remaining == 0;
            let fragment = TransactionFragment {
                header: self.first_header.clone(),
                payload: first,
                offset: 0,
                is_last,
            };
            self.offset = self.first_header.data_size;
            return Ok(Some(fragment));
        }

        if self.remaining == 0 {
            return Ok(None);
        }

        let (next_hdr, chunk) = read_frame(self.reader, self.timeout, self.max_total).await?;
        if !headers_match(&self.first_header, &next_hdr) {
            return Err(TransactionError::SizeMismatch);
        }
        if next_hdr.data_size == 0 || next_hdr.data_size > self.remaining {
            return Err(TransactionError::SizeMismatch);
        }

        let offset = self.offset;
        self.remaining -= next_hdr.data_size;
        self.offset += next_hdr.data_size;

        Ok(Some(TransactionFragment {
            header: next_hdr,
            payload: chunk,
            offset,
            is_last: self.remaining == 0,
        }))
    }
}

/// Reader that yields transactions as streams of fragments.
pub struct TransactionStreamReader<R> {
    reader: R,
    timeout: Duration,
    max_total: usize,
}

impl<R> TransactionStreamReader<R>
where
    R: AsyncRead + Unpin,
{
    /// Create a new streaming reader with default limits.
    #[must_use]
    pub const fn new(reader: R) -> Self {
        Self {
            reader,
            timeout: default_timeout(),
            max_total: MAX_PAYLOAD_SIZE,
        }
    }

    /// Override the I/O timeout used for streaming reads.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the maximum accepted total size for a streamed payload.
    #[must_use]
    pub const fn with_max_total(mut self, max_total: usize) -> Self {
        self.max_total = max_total;
        self
    }

    /// Start reading the next transaction from the underlying reader.
    ///
    /// # Errors
    /// Returns an error if the first frame is invalid or exceeds limits.
    pub async fn start_transaction(
        &mut self,
    ) -> Result<StreamingTransaction<'_, R>, TransactionError> {
        let (first_hdr, first_chunk, remaining) =
            validate_first_frame(&mut self.reader, self.timeout, self.max_total).await?;
        Ok(StreamingTransaction {
            reader: &mut self.reader,
            first_header: first_hdr,
            timeout: self.timeout,
            remaining,
            offset: 0,
            pending_first: Some(first_chunk),
            max_total: self.max_total,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use rstest::rstest;
    use tokio::io::BufReader;

    use super::*;
    use crate::wireframe::test_helpers::{
        fragmented_transaction_bytes,
        mismatched_continuation_bytes,
    };

    #[rstest]
    #[tokio::test]
    async fn streams_large_fragmented_payload() {
        let total = 2 * 1024 * 1024usize;
        let payload = vec![0u8; total];
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 410,
            id: 7,
            error: 0,
            total_size: u32::try_from(total).expect("total fits in u32"),
            data_size: u32::try_from(total).expect("data fits in u32"),
        };
        let fragments =
            fragmented_transaction_bytes(&header, &payload, 32 * 1024).expect("fragment");
        let bytes: Vec<u8> = fragments.into_iter().flatten().collect();

        let mut reader =
            TransactionReader::new(BufReader::new(Cursor::new(bytes))).with_max_payload(total + 1);
        let mut stream = reader.read_streaming_transaction().await.expect("stream");

        let mut seen = 0usize;
        while let Some(fragment) = stream.next_fragment().await.expect("fragment") {
            seen += fragment.payload.len();
            assert!(fragment.payload.len() <= 32 * 1024);
        }

        assert_eq!(seen, total);
    }

    #[rstest]
    #[tokio::test]
    async fn rejects_mismatched_continuation_headers() {
        let bytes = mismatched_continuation_bytes().expect("bytes");
        let mut reader = TransactionReader::new(BufReader::new(Cursor::new(bytes)));
        let mut stream = reader.read_streaming_transaction().await.expect("stream");

        let _first = stream
            .next_fragment()
            .await
            .expect("first")
            .expect("first fragment");
        let err = stream
            .next_fragment()
            .await
            .expect_err("second fragment should fail");

        assert!(matches!(err, TransactionError::SizeMismatch));
    }

    #[rstest]
    #[tokio::test]
    async fn rejects_total_exceeding_stream_limit() {
        let payload = vec![0u8; 10];
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: 10,
            data_size: 10,
        };
        let fragments = fragmented_transaction_bytes(&header, &payload, 10).expect("fragments");
        let bytes: Vec<u8> = fragments.into_iter().flatten().collect();
        let mut reader =
            TransactionStreamReader::new(BufReader::new(Cursor::new(bytes))).with_max_total(5);

        let result = reader.start_transaction().await;
        assert!(result.is_err(), "limit should be exceeded");
        let err = result.err().expect("error present");
        assert!(matches!(err, TransactionError::PayloadTooLarge));
    }
}
