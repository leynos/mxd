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
        super::validate_header(&header)
            .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

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
            super::validate_fragment_consistency(&state.first_header, &header)
                .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

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

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(item) = self.decode(src)? {
            return Ok(Some(item));
        }
        if self.reassembly.is_some() || !src.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete transaction frame",
            ));
        }
        Ok(None)
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

#[cfg(test)]
#[path = "framed_tests.rs"]
mod tests;
