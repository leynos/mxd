use std::collections::HashSet;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

/// Number of bytes in the client handshake message.
pub const HANDSHAKE_LEN: usize = 12;
/// Number of bytes in the server handshake reply.
pub const REPLY_LEN: usize = 8;
/// Fixed protocol identifier used in the Hotline protocol.
pub const PROTOCOL_ID: &[u8; 4] = b"TRTP";
/// Protocol version supported by this server.
pub const VERSION: u16 = 1;

/// Handshake reply code for success.
pub const HANDSHAKE_OK: u32 = 0;
/// Error code for an invalid protocol identifier.
pub const HANDSHAKE_ERR_INVALID: u32 = 1;
/// Error code for an unsupported protocol version.
pub const HANDSHAKE_ERR_UNSUPPORTED_VERSION: u32 = 2;
/// Error code when the handshake times out.
pub const HANDSHAKE_ERR_TIMEOUT: u32 = 3;

/// Timeout for reading the client handshake.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Number of bytes in a transaction frame header.
#[allow(dead_code)]
pub const HEADER_LEN: usize = 20;
/// Maximum allowed total size of a transaction payload.
#[allow(dead_code)]
pub const MAX_TOTAL_SIZE: u32 = 1_048_576; // 1 MiB

/// Parsed handshake information.
#[derive(Debug, PartialEq, Eq)]
pub struct Handshake {
    /// Application-specific sub-protocol identifier.
    pub sub_protocol: u32,
    /// Protocol version number.
    pub version: u16,
    /// Application-defined sub-version number.
    pub sub_version: u16,
}

/// Errors that can occur when parsing a handshake.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HandshakeError {
    #[error("invalid protocol id")]
    InvalidProtocol,
    #[error("unsupported version {0}")]
    UnsupportedVersion(u16),
}

/// Parse the 12-byte client handshake message.
pub fn parse_handshake(buf: &[u8; HANDSHAKE_LEN]) -> Result<Handshake, HandshakeError> {
    if &buf[0..4] != PROTOCOL_ID {
        return Err(HandshakeError::InvalidProtocol);
    }
    let sub_protocol = u32::from_be_bytes(buf[4..8].try_into().unwrap());
    let version = u16::from_be_bytes(buf[8..10].try_into().unwrap());
    let sub_version = u16::from_be_bytes(buf[10..12].try_into().unwrap());
    if version != VERSION {
        return Err(HandshakeError::UnsupportedVersion(version));
    }
    Ok(Handshake {
        sub_protocol,
        version,
        sub_version,
    })
}

/// Convert a [`HandshakeError`] into a numeric error code for clients.
pub fn handshake_error_code(err: &HandshakeError) -> u32 {
    match err {
        HandshakeError::InvalidProtocol => HANDSHAKE_ERR_INVALID,
        HandshakeError::UnsupportedVersion(_) => HANDSHAKE_ERR_UNSUPPORTED_VERSION,
    }
}

/// Write the handshake reply with the provided error code.
///
/// The reply consists of the protocol identifier followed by a 32-bit
/// error code. [`HANDSHAKE_OK`] indicates success, while the other
/// `HANDSHAKE_ERR_*` constants specify why the handshake failed.
pub async fn write_handshake_reply<W>(writer: &mut W, error_code: u32) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut buf = [0u8; REPLY_LEN];
    buf[0..4].copy_from_slice(PROTOCOL_ID);
    buf[4..8].copy_from_slice(&error_code.to_be_bytes());
    writer.write_all(&buf).await
}

/// Parsed transaction frame header.
#[derive(Debug, PartialEq, Eq)]
pub struct FrameHeader {
    /// Reserved flags (must be zero).
    pub flags: u8,
    /// Indicates whether this is a request (`false`) or reply (`true`).
    pub is_reply: bool,
    /// Transaction type identifier.
    pub ty: u16,
    /// Client-chosen transaction ID.
    pub id: u32,
    /// Error code (only meaningful for replies).
    pub error: u32,
    /// Total parameter size across all fragments.
    pub total_size: u32,
    /// Size of the parameter bytes in this fragment.
    pub data_size: u32,
}

/// A parsed transaction parameter.
#[derive(Debug, PartialEq, Eq)]
pub struct Parameter {
    /// Field identifier.
    pub id: u16,
    /// Raw value bytes.
    pub data: Vec<u8>,
}

/// Complete transaction frame with header and parameters.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub header: FrameHeader,
    pub parameters: Vec<Parameter>,
}

/// Errors that can occur while reading or parsing a frame.
#[derive(Debug, Error)]
pub enum FrameError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid flags {0:#x}")]
    InvalidFlags(u8),
    #[error("invalid reply marker {0}")]
    InvalidIsReply(u8),
    #[error("transaction id cannot be zero")]
    ZeroId,
    #[error("frame too large: {0} bytes")]
    FrameTooLarge(u32),
    #[error("data size exceeds total size")]
    DataSizeExceedsTotal,
    #[error("data size mismatch")]
    DataSizeMismatch,
    #[error("fragment header mismatch")]
    FragmentMismatch,
    #[error("duplicate field id {0:#x}")]
    DuplicateField(u16),
    #[error("payload too short")]
    PayloadTooShort,
    #[error("unexpected trailing data")]
    TrailingData,
}

/// Parse a 20-byte transaction frame header.
#[allow(dead_code)]
pub fn parse_frame_header(buf: &[u8; HEADER_LEN]) -> Result<FrameHeader, FrameError> {
    let flags = buf[0];
    if flags != 0 {
        return Err(FrameError::InvalidFlags(flags));
    }
    let is_reply = match buf[1] {
        0 => false,
        1 => true,
        other => return Err(FrameError::InvalidIsReply(other)),
    };
    let ty = u16::from_be_bytes(buf[2..4].try_into().unwrap());
    let id = u32::from_be_bytes(buf[4..8].try_into().unwrap());
    if id == 0 {
        return Err(FrameError::ZeroId);
    }
    let error = u32::from_be_bytes(buf[8..12].try_into().unwrap());
    let total_size = u32::from_be_bytes(buf[12..16].try_into().unwrap());
    if total_size > MAX_TOTAL_SIZE {
        return Err(FrameError::FrameTooLarge(total_size));
    }
    let data_size = u32::from_be_bytes(buf[16..20].try_into().unwrap());
    if data_size > total_size {
        return Err(FrameError::DataSizeExceedsTotal);
    }
    Ok(FrameHeader {
        flags,
        is_reply,
        ty,
        id,
        error,
        total_size,
        data_size,
    })
}

#[allow(dead_code)]
fn parse_parameters(buf: &[u8]) -> Result<Vec<Parameter>, FrameError> {
    if buf.is_empty() {
        return Ok(Vec::new());
    }
    if buf.len() < 2 {
        return Err(FrameError::PayloadTooShort);
    }
    let mut pos = 0usize;
    let count = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    pos += 2;
    let mut params = Vec::with_capacity(count);
    let mut seen = HashSet::new();
    for _ in 0..count {
        if pos + 4 > buf.len() {
            return Err(FrameError::PayloadTooShort);
        }
        let id = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let size = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]) as usize;
        pos += 4;
        if pos + size > buf.len() {
            return Err(FrameError::PayloadTooShort);
        }
        if !seen.insert(id) {
            return Err(FrameError::DuplicateField(id));
        }
        params.push(Parameter {
            id,
            data: buf[pos..pos + size].to_vec(),
        });
        pos += size;
    }
    if pos != buf.len() {
        return Err(FrameError::TrailingData);
    }
    Ok(params)
}

/// Read a complete transaction frame from the provided reader.
#[allow(dead_code)]
pub async fn read_frame<R>(reader: &mut R) -> Result<Frame, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    let mut hdr_buf = [0u8; HEADER_LEN];
    reader.read_exact(&mut hdr_buf).await?;
    let header = parse_frame_header(&hdr_buf)?;

    let mut data = Vec::with_capacity(header.total_size as usize);
    if header.data_size > 0 {
        let mut tmp = vec![0u8; header.data_size as usize];
        reader.read_exact(&mut tmp).await?;
        data.extend_from_slice(&tmp);
    }

    let mut remaining = header.total_size - header.data_size;
    while remaining > 0 {
        reader.read_exact(&mut hdr_buf).await?;
        let frag = parse_frame_header(&hdr_buf)?;
        if frag.flags != header.flags
            || frag.is_reply != header.is_reply
            || frag.ty != header.ty
            || frag.id != header.id
            || frag.error != header.error
            || frag.total_size != header.total_size
        {
            return Err(FrameError::FragmentMismatch);
        }
        if frag.data_size > remaining {
            return Err(FrameError::DataSizeExceedsTotal);
        }
        let mut tmp = vec![0u8; frag.data_size as usize];
        reader.read_exact(&mut tmp).await?;
        data.extend_from_slice(&tmp);
        remaining -= frag.data_size;
    }

    if data.len() != header.total_size as usize {
        return Err(FrameError::DataSizeMismatch);
    }

    let params = parse_parameters(&data)?;
    Ok(Frame {
        header,
        parameters: params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn build_single_frame() -> Vec<u8> {
        let mut buf = Vec::new();
        let payload: [u8; 9] = [
            0x00, 0x01, // param count
            0x00, 0x66, // field id
            0x00, 0x03, // field size
            0xde, 0xad, 0xbe,
        ];
        buf.extend_from_slice(&[
            0, // flags
            0, // is_reply
            0x01, 0x23, // type
        ]);
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&payload);
        buf
    }

    fn build_fragmented_frame() -> Vec<u8> {
        let mut buf = Vec::new();
        let payload: [u8; 9] = [
            0x00, 0x01, // param count
            0x00, 0x66, // field id
            0x00, 0x03, // field size
            0xde, 0xad, 0xbe,
        ];

        // first fragment header
        buf.extend_from_slice(&[0, 0, 0x01, 0x23]);
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&4u32.to_be_bytes());
        buf.extend_from_slice(&payload[..4]);

        // second fragment header
        buf.extend_from_slice(&[0, 0, 0x01, 0x23]);
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&(payload.len() as u32 - 4).to_be_bytes());
        buf.extend_from_slice(&payload[4..]);

        buf
    }

    #[tokio::test]
    async fn read_single_frame() {
        let data = build_single_frame();
        let mut cur = Cursor::new(data);
        let frame = read_frame(&mut cur).await.unwrap();
        assert_eq!(frame.header.ty, 0x0123);
        assert_eq!(frame.parameters.len(), 1);
        assert_eq!(frame.parameters[0].id, 0x0066);
        assert_eq!(frame.parameters[0].data, vec![0xde, 0xad, 0xbe]);
    }

    #[tokio::test]
    async fn read_fragmented_frame() {
        let data = build_fragmented_frame();
        let mut cur = Cursor::new(data);
        let frame = read_frame(&mut cur).await.unwrap();
        assert_eq!(frame.header.ty, 0x0123);
        assert_eq!(frame.parameters.len(), 1);
        assert_eq!(frame.parameters[0].id, 0x0066);
    }

    #[tokio::test]
    async fn reject_invalid_flags() {
        let mut frame = build_single_frame();
        frame[0] = 1; // set flags to non-zero
        let mut cur = Cursor::new(frame);
        let err = read_frame(&mut cur).await.unwrap_err();
        assert!(matches!(err, FrameError::InvalidFlags(1)));
    }

    #[test]
    fn parse_valid_handshake() {
        let mut buf = [0u8; HANDSHAKE_LEN];
        buf[0..4].copy_from_slice(PROTOCOL_ID);
        buf[8..10].copy_from_slice(&VERSION.to_be_bytes());
        let hs = parse_handshake(&buf).unwrap();
        assert_eq!(
            hs,
            Handshake {
                sub_protocol: 0,
                version: VERSION,
                sub_version: 0
            }
        );
    }

    #[test]
    fn reject_invalid_protocol() {
        let mut buf = [0u8; HANDSHAKE_LEN];
        buf[0..4].copy_from_slice(b"WRNG");
        buf[8..10].copy_from_slice(&VERSION.to_be_bytes());
        assert!(matches!(
            parse_handshake(&buf),
            Err(HandshakeError::InvalidProtocol)
        ));
        assert_eq!(
            handshake_error_code(&HandshakeError::InvalidProtocol),
            HANDSHAKE_ERR_INVALID
        );
    }

    #[test]
    fn reject_bad_version() {
        let mut buf = [0u8; HANDSHAKE_LEN];
        buf[0..4].copy_from_slice(PROTOCOL_ID);
        buf[8..10].copy_from_slice(&2u16.to_be_bytes());
        assert!(matches!(
            parse_handshake(&buf),
            Err(HandshakeError::UnsupportedVersion(2))
        ));
        assert_eq!(
            handshake_error_code(&HandshakeError::UnsupportedVersion(2)),
            HANDSHAKE_ERR_UNSUPPORTED_VERSION
        );
    }
}
