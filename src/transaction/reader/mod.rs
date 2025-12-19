//! Buffered and streaming readers for Hotline transactions.
//!
//! [`TransactionReader`] assembles entire payloads into memory for small
//! request/response frames. [`TransactionStreamReader`] exposes an incremental
//! interface that yields payload fragments without buffering the full message,
//! enabling large file transfers to be processed safely.

mod streaming;

use std::time::Duration;

use tokio::io::AsyncRead;

pub use self::streaming::{StreamingTransaction, TransactionFragment, TransactionStreamReader};
use self::streaming::{
    StreamingTransactionInit,
    build_streaming_transaction,
    validate_continuation_frame,
    validate_first_frame,
};
use super::{
    FrameHeader,
    IO_TIMEOUT,
    MAX_PAYLOAD_SIZE,
    Transaction,
    errors::TransactionError,
    frame::read_frame,
    params::validate_payload,
};

/// Check whether a continuation frame header matches the first frame header.
pub(crate) const fn headers_match(first: &FrameHeader, next: &FrameHeader) -> bool {
    next.ty == first.ty
        && next.id == first.id
        && next.error == first.error
        && next.total_size == first.total_size
        && next.flags == first.flags
        && next.is_reply == first.is_reply
}

/// Buffered reader for Hotline transactions.
///
/// Assembles multi-fragment transactions into a complete in-memory
/// [`Transaction`]. For large payloads, use
/// [`read_streaming_transaction`](Self::read_streaming_transaction) to obtain
/// a [`StreamingTransaction`] that yields fragments incrementally.
pub struct TransactionReader<R> {
    reader: R,
    timeout: Duration,
    max_payload: usize,
}

/// Validate the first frame header of a transaction.
pub(crate) const fn validate_first_header(
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
        let (mut header, mut payload) =
            read_frame(&mut self.reader, self.timeout, self.max_payload).await?;
        validate_first_header(&header, self.max_payload)?;

        let mut remaining = header.total_size - header.data_size;
        while remaining > 0 {
            let (next_hdr, chunk) =
                read_frame(&mut self.reader, self.timeout, self.max_payload).await?;
            validate_continuation_frame(&header, &next_hdr, remaining)?;
            payload.extend_from_slice(&chunk);
            remaining -= next_hdr.data_size;
        }

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
        Ok(build_streaming_transaction(
            &mut self.reader,
            StreamingTransactionInit {
                first_hdr,
                first_chunk,
                remaining,
                timeout: self.timeout,
                max_total: self.max_payload,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tokio::io::BufReader;

    use super::*;
    use crate::wireframe::test_helpers::{
        fragmented_transaction_bytes,
        mismatched_continuation_bytes,
    };

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

        assert!(matches!(err, TransactionError::HeaderMismatch));
    }

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
