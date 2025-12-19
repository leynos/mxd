//! Streaming readers for large transaction payloads.
//!
//! These types enable incremental processing of transaction payloads without
//! buffering the entire content in memory, suitable for file transfers.

use std::time::Duration;

use tokio::io::AsyncRead;

use super::{headers_match, validate_first_header};
use crate::transaction::{
    FrameHeader,
    MAX_PAYLOAD_SIZE,
    errors::TransactionError,
    frame::{default_timeout, read_frame},
};

/// Initialisation data for constructing a `StreamingTransaction`.
pub(super) struct StreamingTransactionInit {
    pub first_hdr: FrameHeader,
    pub first_chunk: Vec<u8>,
    pub remaining: u32,
    pub timeout: Duration,
    pub max_total: usize,
}

/// Validate and read the first frame of a streaming transaction.
pub(super) async fn validate_first_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    timeout: Duration,
    max_limit: usize,
) -> Result<(FrameHeader, Vec<u8>, u32), TransactionError> {
    let (first_hdr, first_chunk) = read_frame(reader, timeout, max_limit).await?;

    // Delegate to shared header validation to keep invariants in one place.
    validate_first_header(&first_hdr, max_limit)?;

    let remaining = first_hdr.total_size - first_hdr.data_size;
    Ok((first_hdr, first_chunk, remaining))
}

/// Construct a `StreamingTransaction` from validated first-frame data.
pub(super) fn build_streaming_transaction<R: AsyncRead + Unpin>(
    reader: &mut R,
    init: StreamingTransactionInit,
) -> StreamingTransaction<'_, R> {
    StreamingTransaction {
        reader,
        first_header: init.first_hdr,
        timeout: init.timeout,
        remaining: init.remaining,
        offset: 0,
        pending_first: Some(init.first_chunk),
        max_total: init.max_total,
    }
}

/// Validate a continuation frame against the first frame header.
pub(super) const fn validate_continuation_frame(
    first: &FrameHeader,
    next: &FrameHeader,
    remaining: u32,
) -> Result<(), TransactionError> {
    if !headers_match(first, next) {
        return Err(TransactionError::HeaderMismatch);
    }
    if next.data_size == 0 || next.data_size > remaining {
        return Err(TransactionError::SizeMismatch);
    }
    Ok(())
}

/// A single payload fragment from a multi-frame transaction.
///
/// Returned by [`StreamingTransaction::next_fragment`] to deliver payload
/// chunks incrementally without buffering the full message.
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

/// Incremental view over a multi-fragment transaction.
///
/// Returned by [`TransactionReader::read_streaming_transaction`] or
/// [`TransactionStreamReader::start_transaction`]. Call
/// [`next_fragment`](Self::next_fragment) repeatedly to consume the payload
/// without buffering the entire content in memory.
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
        validate_continuation_frame(&self.first_header, &next_hdr, self.remaining)?;

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

/// Streaming reader for large transaction payloads.
///
/// Unlike [`TransactionReader`](super::TransactionReader), this reader exposes
/// an incremental interface via [`start_transaction`](Self::start_transaction),
/// yielding payload fragments without buffering the full message. Suitable for
/// file transfers and other large payloads.
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
        Ok(build_streaming_transaction(
            &mut self.reader,
            StreamingTransactionInit {
                first_hdr,
                first_chunk,
                remaining,
                timeout: self.timeout,
                max_total: self.max_total,
            },
        ))
    }
}
