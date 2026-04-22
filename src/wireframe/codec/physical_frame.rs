//! Shared helpers for decoding one physical Hotline frame from a byte buffer.

use std::io;

use bytes::{Buf, BytesMut};

use crate::transaction::{FrameHeader, HEADER_LEN};

/// Try to read one complete physical Hotline frame from `src`.
///
/// Returns `Ok(None)` when more bytes are required, or the validated header
/// plus payload chunk once a full frame is available.
pub(crate) fn take_hotline_frame(
    src: &mut BytesMut,
) -> Result<Option<(FrameHeader, Vec<u8>)>, io::Error> {
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
    let payload = src.split_to(data_size).to_vec();
    Ok(Some((header, payload)))
}
