//! Wireframe codec for Hotline transaction framing.
//!
//! This module implements `BorrowDecode` for transaction frames, enabling the
//! wireframe transport to decode the 20-byte header and reassemble fragmented
//! payloads according to `docs/protocol.md`.

use bincode::{
    de::{BorrowDecode, BorrowDecoder, read::Reader},
    error::DecodeError,
};

use crate::transaction::{FrameHeader, HEADER_LEN, MAX_FRAME_DATA, MAX_PAYLOAD_SIZE};

/// Wireframe-decoded Hotline transaction.
///
/// Wraps the validated header and reassembled payload after decoding from the
/// wireframe transport layer.
///
/// **Note:** After multi-fragment reassembly, the header's `data_size` field is
/// set to `total_size` to reflect the fully-assembled payload length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotlineTransaction {
    header: FrameHeader,
    payload: Vec<u8>,
}

impl HotlineTransaction {
    /// Return the transaction header.
    #[must_use]
    pub const fn header(&self) -> &FrameHeader { &self.header }

    /// Return the reassembled payload.
    #[must_use]
    pub fn payload(&self) -> &[u8] { &self.payload }

    /// Consume self and return the inner header and payload.
    #[must_use]
    pub fn into_parts(self) -> (FrameHeader, Vec<u8>) { (self.header, self.payload) }
}

/// Validate a frame header against protocol constraints.
///
/// # Errors
///
/// Returns a descriptive error string if validation fails.
const fn validate_header(hdr: &FrameHeader) -> Result<(), &'static str> {
    if hdr.flags != 0 {
        return Err("invalid flags: must be 0 for v1.8.5");
    }
    if hdr.total_size as usize > MAX_PAYLOAD_SIZE {
        return Err("total size exceeds maximum (1 MiB)");
    }
    if hdr.data_size as usize > MAX_FRAME_DATA {
        return Err("data size exceeds maximum (32 KiB)");
    }
    if hdr.data_size > hdr.total_size {
        return Err("data size exceeds total size");
    }
    // Empty data with non-zero total is invalid (except for single empty frame)
    if hdr.data_size == 0 && hdr.total_size > 0 {
        return Err("data size is zero but total size is non-zero");
    }
    Ok(())
}

/// Validate that a continuation fragment has consistent header fields.
///
/// # Errors
///
/// Returns a descriptive error string if header fields mutated between fragments.
const fn validate_fragment_consistency(
    first: &FrameHeader,
    next: &FrameHeader,
) -> Result<(), &'static str> {
    if next.flags != first.flags {
        return Err("header mismatch: 'flags' changed between fragments");
    }
    if next.is_reply != first.is_reply {
        return Err("header mismatch: 'is_reply' changed between fragments");
    }
    if next.ty != first.ty {
        return Err("header mismatch: 'type' changed between fragments");
    }
    if next.id != first.id {
        return Err("header mismatch: 'id' changed between fragments");
    }
    if next.error != first.error {
        return Err("header mismatch: 'error' changed between fragments");
    }
    if next.total_size != first.total_size {
        return Err("header mismatch: 'total_size' changed between fragments");
    }
    Ok(())
}

impl<'de> BorrowDecode<'de, ()> for HotlineTransaction {
    fn borrow_decode<D: BorrowDecoder<'de, Context = ()>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        // Read the first frame header
        let mut hdr_buf = [0u8; HEADER_LEN];
        decoder.reader().read(&mut hdr_buf)?;
        let first_header = FrameHeader::from_bytes(&hdr_buf);

        // Validate header constraints
        validate_header(&first_header).map_err(|msg| DecodeError::OtherString(msg.to_owned()))?;

        // Read the first fragment's data
        let mut payload = vec![0u8; first_header.data_size as usize];
        if !payload.is_empty() {
            decoder.reader().read(&mut payload)?;
        }

        // Handle multi-fragment case
        let mut accumulated = first_header.data_size;
        while accumulated < first_header.total_size {
            // Read continuation header
            decoder.reader().read(&mut hdr_buf)?;
            let next_header = FrameHeader::from_bytes(&hdr_buf);

            // Validate continuation header
            validate_fragment_consistency(&first_header, &next_header)
                .map_err(|msg| DecodeError::OtherString(msg.to_owned()))?;

            // Validate data size for continuation
            if next_header.data_size == 0 {
                return Err(DecodeError::OtherString(
                    "continuation fragment has zero data size".to_owned(),
                ));
            }
            if next_header.data_size as usize > MAX_FRAME_DATA {
                return Err(DecodeError::OtherString(
                    "continuation data size exceeds maximum (32 KiB)".to_owned(),
                ));
            }

            let remaining = first_header.total_size - accumulated;
            if next_header.data_size > remaining {
                return Err(DecodeError::OtherString(
                    "continuation data size exceeds remaining bytes".to_owned(),
                ));
            }

            // Read continuation data directly into payload
            let chunk_size = next_header.data_size as usize;
            let start = payload.len();
            payload.resize(start + chunk_size, 0);
            let chunk = payload.get_mut(start..start + chunk_size).ok_or_else(|| {
                DecodeError::OtherString("payload resize failed for continuation".to_owned())
            })?;
            decoder.reader().read(chunk)?;
            accumulated += next_header.data_size;
        }

        // Build the final transaction with data_size = total_size (fully assembled)
        let mut header = first_header;
        header.data_size = header.total_size;

        // Note: Payload validation (transaction::validate_payload) is intentionally
        // omitted here. The codec layer handles only frame-level concerns—header
        // validation, length bounds, and multi-fragment reassembly. Semantic payload
        // validation (parameter counts, field types) is deferred to command handlers,
        // which have the context to interpret transaction types and enforce
        // application-level constraints.
        Ok(Self { header, payload })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use rstest::rstest;
    use tokio::io::BufReader;
    use wireframe::preamble::read_preamble;

    use super::*;
    use crate::wireframe::test_helpers::transaction_bytes;

    #[rstest]
    #[case(20, 20)] // Single frame with payload
    #[case(0, 0)] // Empty payload
    #[tokio::test]
    async fn decodes_valid_single_frame(#[case] total: u32, #[case] data: u32) {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: total,
            data_size: data,
        };
        let payload = vec![0u8; total as usize];
        let bytes = transaction_bytes(&header, &payload);
        let mut reader = BufReader::new(Cursor::new(bytes));

        let (tx, leftover) = read_preamble::<_, HotlineTransaction>(&mut reader)
            .await
            .expect("transaction must decode");

        assert!(leftover.is_empty());
        assert_eq!(tx.header().total_size, total);
        assert_eq!(tx.payload().len(), total as usize);
    }

    #[rstest]
    #[case(10, 20, "data size exceeds total")]
    #[case(100, 0, "data size is zero but total size is non-zero")]
    #[tokio::test]
    async fn rejects_invalid_length_combinations(
        #[case] total: u32,
        #[case] data: u32,
        #[case] expected_msg: &str,
    ) {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: total,
            data_size: data,
        };
        let payload = vec![0u8; data as usize];
        let bytes = transaction_bytes(&header, &payload);
        let mut reader = BufReader::new(Cursor::new(bytes));

        let err = read_preamble::<_, HotlineTransaction>(&mut reader)
            .await
            .expect_err("decode must fail");

        assert!(
            err.to_string().contains(expected_msg),
            "expected '{expected_msg}' in '{err}'"
        );
    }

    #[tokio::test]
    async fn rejects_invalid_flags() {
        let header = FrameHeader {
            flags: 1,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: 0,
            data_size: 0,
        };
        let bytes = transaction_bytes(&header, &[]);
        let mut reader = BufReader::new(Cursor::new(bytes));

        let err = read_preamble::<_, HotlineTransaction>(&mut reader)
            .await
            .expect_err("decode must fail");

        assert!(
            err.to_string().contains("invalid flags"),
            "expected 'invalid flags' in '{err}'"
        );
    }

    #[tokio::test]
    async fn rejects_oversized_total() {
        let oversized_total = u32::try_from(MAX_PAYLOAD_SIZE + 1).expect("test size fits in u32");
        let frame_data = u32::try_from(MAX_FRAME_DATA).expect("frame data fits in u32");
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: oversized_total,
            data_size: frame_data,
        };
        let payload = vec![0u8; MAX_FRAME_DATA];
        let bytes = transaction_bytes(&header, &payload);
        let mut reader = BufReader::new(Cursor::new(bytes));

        let err = read_preamble::<_, HotlineTransaction>(&mut reader)
            .await
            .expect_err("decode must fail");

        assert!(
            err.to_string().contains("total size exceeds maximum"),
            "expected 'total size exceeds maximum' in '{err}'"
        );
    }

    #[tokio::test]
    async fn rejects_oversized_data() {
        let oversized = u32::try_from(MAX_FRAME_DATA + 1).expect("test size fits in u32");
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: oversized,
            data_size: oversized,
        };
        let payload = vec![0u8; MAX_FRAME_DATA + 1];
        let bytes = transaction_bytes(&header, &payload);
        let mut reader = BufReader::new(Cursor::new(bytes));

        let err = read_preamble::<_, HotlineTransaction>(&mut reader)
            .await
            .expect_err("decode must fail");

        assert!(
            err.to_string().contains("data size exceeds maximum"),
            "expected 'data size exceeds maximum' in '{err}'"
        );
    }
}
