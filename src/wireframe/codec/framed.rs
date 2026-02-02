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

    fn peek_header(src: &BytesMut) -> Result<Option<FrameHeader>, io::Error> {
        if src.len() < HEADER_LEN {
            return Ok(None);
        }

        let header_slice = src
            .get(..HEADER_LEN)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "missing header bytes"))?;
        let header_bytes: &[u8; HEADER_LEN] = header_slice
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid header length"))?;
        let header = FrameHeader::from_bytes(header_bytes);

        super::validate_header(&header)
            .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

        Ok(Some(header))
    }

    fn take_frame_payload(
        src: &mut BytesMut,
        header: &FrameHeader,
    ) -> Result<Option<Vec<u8>>, io::Error> {
        let data_size = usize::try_from(header.data_size)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame data size too large"))?;
        let frame_len = HEADER_LEN
            .checked_add(data_size)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "frame length overflow"))?;
        if src.len() < frame_len {
            src.reserve(frame_len - src.len());
            return Ok(None);
        }

        src.advance(HEADER_LEN);
        Ok(Some(src.split_to(data_size).to_vec()))
    }

    fn finalize_transaction(
        header: FrameHeader,
        payload: Vec<u8>,
    ) -> Result<HotlineTransaction, io::Error> {
        let mut final_header = header;
        final_header.data_size = final_header.total_size;
        HotlineTransaction::from_parts(final_header, payload)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
    }

    fn append_fragment(
        state: &mut ReassemblyState,
        header: &FrameHeader,
        payload: &[u8],
    ) -> Result<bool, io::Error> {
        super::validate_fragment_consistency(&state.first_header, header)
            .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

        if header.data_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "continuation fragment has zero data size",
            ));
        }

        let total_size = usize::try_from(state.first_header.total_size).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "frame total size too large")
        })?;
        let data_size = usize::try_from(header.data_size)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame data size too large"))?;
        let remaining = total_size.checked_sub(state.payload.len()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "reassembly payload exceeds declared total size",
            )
        })?;
        if data_size > remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fragment exceeds remaining payload size",
            ));
        }

        state.payload.extend_from_slice(payload);

        Ok(state.payload.len() == total_size)
    }

    fn missing_reassembly_state() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, "missing reassembly state")
    }
}

impl Decoder for HotlineCodec {
    type Error = io::Error;
    type Item = HotlineTransaction;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let Some(header) = Self::peek_header(src)? else {
            return Ok(None);
        };
        let Some(payload) = Self::take_frame_payload(src, &header)? else {
            return Ok(None);
        };

        if let Some(state) = self.reassembly.as_mut() {
            let is_complete = Self::append_fragment(state, &header, &payload)?;
            if !is_complete {
                return Ok(None);
            }
            let completed_state = self
                .reassembly
                .take()
                .ok_or_else(Self::missing_reassembly_state)?;
            let tx =
                Self::finalize_transaction(completed_state.first_header, completed_state.payload)?;
            return Ok(Some(tx));
        }

        if header.data_size < header.total_size {
            self.reassembly = Some(ReassemblyState {
                first_header: header,
                payload,
            });
            return Ok(None);
        }

        Self::finalize_transaction(header, payload).map(Some)
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
            // Offset starts at 0, increments by chunk size, and end is capped at payload.len().
            let chunk = payload.get(offset..end).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "payload chunk out of bounds")
            })?;
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
