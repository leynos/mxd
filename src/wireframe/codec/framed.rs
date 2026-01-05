//! Tokio codec adapter for Hotline transaction framing.
//!
//! This module provides a [`HotlineCodec`] that implements Tokio's [`Decoder`]
//! and [`Encoder`] traits, enabling integration with [`tokio_util::codec::Framed`]
//! for TCP stream handling. Unlike the wireframe library's default length-delimited
//! codec, this codec correctly handles Hotline's 20-byte header framing format.
//!
//! # Usage
//!
//! ```rust,ignore
//! use tokio::net::TcpStream;
//! use tokio_util::codec::Framed;
//! use mxd::wireframe::codec::HotlineCodec;
//!
//! async fn handle_connection(stream: TcpStream) {
//!     let mut framed = Framed::new(stream, HotlineCodec::new());
//!     // Use framed.next() and framed.send() for frame I/O
//! }
//! ```

use std::io;

use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use super::HotlineTransaction;
use crate::transaction::{
    FrameHeader,
    HEADER_LEN,
    MAX_FRAME_DATA,
    MAX_PAYLOAD_SIZE,
    TransactionError,
};

/// Tokio codec for Hotline transaction framing.
///
/// This codec decodes incoming bytes into [`HotlineTransaction`] values and
/// encodes outbound transactions into wire format. It handles multi-fragment
/// reassembly on decode and fragmentation on encode.
///
/// # Frame Format
///
/// Each frame consists of a 20-byte header followed by a variable-length payload:
///
/// | Field       | Offset | Size | Description                           |
/// |-------------|--------|------|---------------------------------------|
/// | flags       | 0      | 1    | Reserved; must be 0 for v1.8.5        |
/// | is_reply    | 1      | 1    | 0 = request, 1 = reply                |
/// | type        | 2      | 2    | Transaction type identifier           |
/// | id          | 4      | 4    | Transaction ID                        |
/// | error       | 8      | 4    | Error code (0 = success)              |
/// | total_size  | 12     | 4    | Total payload size across fragments   |
/// | data_size   | 16     | 4    | Payload size in this frame (≤32 KiB)  |
/// | payload     | 20     | var  | Frame payload (`data_size` bytes)     |
#[derive(Debug, Default)]
pub struct HotlineCodec {
    /// State for multi-fragment reassembly.
    reassembly: Option<ReassemblyState>,
}

/// State for reassembling multi-fragment transactions.
#[derive(Debug)]
struct ReassemblyState {
    /// Header from the first fragment.
    first_header: FrameHeader,
    /// Accumulated payload bytes.
    payload: Vec<u8>,
}

impl HotlineCodec {
    /// Create a new Hotline codec.
    #[must_use]
    pub fn new() -> Self { Self::default() }
}

impl Decoder for HotlineCodec {
    type Error = io::Error;
    type Item = HotlineTransaction;

    #[expect(
        clippy::expect_used,
        reason = "reassembly state is guaranteed to exist when inside the if-let arm"
    )]
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Check if we have enough bytes for a header
        if src.len() < HEADER_LEN {
            return Ok(None);
        }

        // Peek at the header without consuming
        // SAFETY: Length checked at start of function ensures HEADER_LEN bytes exist
        let header_slice = src
            .get(..HEADER_LEN)
            .expect("length verified at function start");
        let header = FrameHeader::from_bytes(
            header_slice
                .try_into()
                .expect("slice length guaranteed by get()"),
        );

        // Validate header
        validate_header(&header)?;

        // Check if we have the full frame
        let frame_len = HEADER_LEN + header.data_size as usize;
        if src.len() < frame_len {
            // Reserve space for the remaining bytes
            src.reserve(frame_len - src.len());
            return Ok(None);
        }

        // Consume the header
        src.advance(HEADER_LEN);

        // Read the payload
        let payload_data = src.split_to(header.data_size as usize).to_vec();

        // Handle reassembly
        if let Some(ref mut state) = self.reassembly {
            // Validate fragment consistency
            validate_fragment_consistency(&state.first_header, &header)?;

            // Validate continuation fragment
            if header.data_size == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continuation fragment has zero data size",
                ));
            }

            let remaining = state.first_header.total_size as usize - state.payload.len();
            if header.data_size as usize > remaining {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fragment exceeds remaining payload size",
                ));
            }

            // Append to accumulated payload
            state.payload.extend_from_slice(&payload_data);

            // Check if complete
            if state.payload.len() == state.first_header.total_size as usize {
                let completed = self.reassembly.take().expect("state exists");
                let mut final_header = completed.first_header;
                final_header.data_size = final_header.total_size;
                let tx = HotlineTransaction::from_parts(final_header, completed.payload)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
                return Ok(Some(tx));
            }

            // Need more fragments
            return Ok(None);
        }

        // First/only fragment
        if header.data_size < header.total_size {
            // Multi-fragment transaction; start reassembly
            self.reassembly = Some(ReassemblyState {
                first_header: header,
                payload: payload_data,
            });
            return Ok(None);
        }

        // Single-frame transaction
        let mut final_header = header;
        final_header.data_size = final_header.total_size;
        let tx = HotlineTransaction::from_parts(final_header, payload_data)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        Ok(Some(tx))
    }
}

impl Encoder<HotlineTransaction> for HotlineCodec {
    type Error = io::Error;

    #[expect(
        clippy::expect_used,
        reason = "slice bounds verified by loop invariant: offset starts at 0, end capped at len()"
    )]
    fn encode(&mut self, item: HotlineTransaction, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let (header, payload) = item.into_parts();

        // Validate
        if header.flags != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                TransactionError::InvalidFlags.to_string(),
            ));
        }
        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                TransactionError::PayloadTooLarge.to_string(),
            ));
        }

        // Handle empty payload
        if payload.is_empty() {
            let mut frame_header = header;
            frame_header.data_size = 0;
            let mut header_bytes = [0u8; HEADER_LEN];
            frame_header.write_bytes(&mut header_bytes);
            dst.reserve(HEADER_LEN);
            dst.put_slice(&header_bytes);
            return Ok(());
        }

        // Fragment if needed
        let mut offset = 0usize;
        while offset < payload.len() {
            let end = (offset + MAX_FRAME_DATA).min(payload.len());
            // SAFETY: offset starts at 0 and increments by chunk size; end is capped at
            // payload.len()
            let chunk = payload
                .get(offset..end)
                .expect("offset and end are within bounds");
            let mut frame_header = header.clone();
            frame_header.data_size = u32::try_from(chunk.len())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "chunk too large"))?;

            dst.reserve(HEADER_LEN + chunk.len());
            let mut header_bytes = [0u8; HEADER_LEN];
            frame_header.write_bytes(&mut header_bytes);
            dst.put_slice(&header_bytes);
            dst.put_slice(chunk);
            offset = end;
        }

        Ok(())
    }
}

/// Validate a frame header against protocol constraints.
fn validate_header(hdr: &FrameHeader) -> Result<(), io::Error> {
    if hdr.flags != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid flags: must be 0 for v1.8.5",
        ));
    }
    if hdr.total_size as usize > MAX_PAYLOAD_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "total size exceeds maximum (1 MiB)",
        ));
    }
    if hdr.data_size as usize > MAX_FRAME_DATA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "data size exceeds maximum (32 KiB)",
        ));
    }
    if hdr.data_size > hdr.total_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "data size exceeds total size",
        ));
    }
    if hdr.data_size == 0 && hdr.total_size > 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "data size is zero but total size is non-zero",
        ));
    }
    Ok(())
}

/// Validate that a continuation fragment has consistent header fields.
fn validate_fragment_consistency(first: &FrameHeader, next: &FrameHeader) -> Result<(), io::Error> {
    if next.flags != first.flags {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "header mismatch: 'flags' changed between fragments",
        ));
    }
    if next.is_reply != first.is_reply {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "header mismatch: 'is_reply' changed between fragments",
        ));
    }
    if next.ty != first.ty {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "header mismatch: 'type' changed between fragments",
        ));
    }
    if next.id != first.id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "header mismatch: 'id' changed between fragments",
        ));
    }
    if next.error != first.error {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "header mismatch: 'error' changed between fragments",
        ));
    }
    if next.total_size != first.total_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "header mismatch: 'total_size' changed between fragments",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::{field_id::FieldId, wireframe::test_helpers::transaction_bytes};

    #[rstest]
    #[case::single_frame(
        HotlineTransaction::request_from_params(
            107,
            1,
            &[
                (FieldId::Login, b"alice".as_slice()),
                (FieldId::Password, b"secret".as_slice()),
            ],
        )
        .expect("request tx"),
        107,
        0,
    )]
    #[case::empty_frame(
        HotlineTransaction::reply_from_params(
            &FrameHeader {
                flags: 0,
                is_reply: 0,
                ty: 200,
                id: 42,
                error: 0,
                total_size: 0,
                data_size: 0,
            },
            0,
            &[] as &[(FieldId, &[u8])],
        )
        .expect("reply tx"),
        200,
        1,
    )]
    fn decodes_frame(
        #[case] tx: HotlineTransaction,
        #[case] expected_ty: u16,
        #[case] expected_is_reply: u8,
    ) {
        let mut codec = HotlineCodec::new();
        let (header, payload) = tx.into_parts();
        let mut buf = BytesMut::from(&transaction_bytes(&header, &payload)[..]);

        let result = codec.decode(&mut buf).expect("decode should succeed");

        let decoded = result.expect("should produce transaction");
        assert_eq!(decoded.header().ty, expected_ty);
        assert_eq!(decoded.header().is_reply, expected_is_reply);
        assert_eq!(decoded.payload(), payload.as_slice());
    }

    #[rstest]
    fn returns_none_for_partial_header() {
        let mut codec = HotlineCodec::new();
        let mut buf = BytesMut::from(&[0u8; 10][..]);

        let result = codec.decode(&mut buf).expect("decode should succeed");

        assert!(result.is_none());
        // Buffer should be unchanged
        assert_eq!(buf.len(), 10);
    }

    #[rstest]
    fn returns_none_for_partial_payload() {
        let mut codec = HotlineCodec::new();
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: 100,
            data_size: 100,
        };
        // Only include header + 10 bytes of payload
        let bytes = transaction_bytes(&header, &[0u8; 10]);
        let mut buf = BytesMut::from(&bytes[..]);

        let result = codec.decode(&mut buf).expect("decode should succeed");

        assert!(result.is_none());
    }

    #[rstest]
    fn encodes_single_frame() {
        let mut codec = HotlineCodec::new();
        let tx = HotlineTransaction::request_from_params(107, 99, &[(FieldId::Login, b"alice")])
            .expect("transaction");
        let payload = tx.payload().to_vec();
        let mut buf = BytesMut::new();

        codec.encode(tx, &mut buf).expect("encode should succeed");

        assert_eq!(buf.len(), HEADER_LEN + payload.len());
        let decoded_header = FrameHeader::from_bytes(
            buf[..HEADER_LEN]
                .try_into()
                .expect("header slice correct size"),
        );
        assert_eq!(decoded_header.ty, 107);
        assert_eq!(&buf[HEADER_LEN..], payload.as_slice());
    }

    #[rstest]
    fn encodes_empty_frame() {
        let mut codec = HotlineCodec::new();
        let tx = HotlineTransaction::from_parts(
            FrameHeader {
                flags: 0,
                is_reply: 1,
                ty: 200,
                id: 1,
                error: 0,
                total_size: 0,
                data_size: 0,
            },
            Vec::new(),
        )
        .expect("transaction");
        let mut buf = BytesMut::new();

        codec.encode(tx, &mut buf).expect("encode should succeed");

        assert_eq!(buf.len(), HEADER_LEN);
    }

    #[rstest]
    fn rejects_invalid_flags() {
        let mut codec = HotlineCodec::new();
        let header = FrameHeader {
            flags: 1,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: 0,
            data_size: 0,
        };
        let mut buf = BytesMut::from(&transaction_bytes(&header, &[])[..]);

        let err = codec.decode(&mut buf).expect_err("decode should fail");

        assert!(err.to_string().contains("invalid flags"));
    }
}
