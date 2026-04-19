//! Hotline-specific protocol message assembly helpers for Wireframe.
//!
//! This adapter keeps Hotline fragment sequencing and logical transaction
//! reconstruction inside the transport layer. It exposes enough metadata for
//! Wireframe's message-assembly subsystem to enforce per-message and
//! per-connection budgets while preserving the downstream `header || payload`
//! byte shape expected by MXD's existing routing path.

use std::io;

use wireframe::message_assembler::{
    ContinuationFrameHeader,
    FirstFrameHeader,
    FrameHeader as AssemblyFrameHeader,
    FrameSequence,
    MessageAssembler,
    MessageKey,
    ParsedFrameHeader,
};

use crate::transaction::{FrameHeader, HEADER_LEN, MAX_PAYLOAD_SIZE};

/// Maximum logical Hotline transaction size carried through the Wireframe app.
///
/// This is a logical request budget, not a physical frame ceiling. Physical
/// Hotline frames remain capped by `MAX_FRAME_DATA`; the extra headroom lets
/// Wireframe size protocol-level message assembly against the full reassembled
/// transaction envelope.
pub(crate) const HOTLINE_LOGICAL_MESSAGE_BYTES: usize = HEADER_LEN + MAX_PAYLOAD_SIZE;

const FIRST_FRAME_TAG: u8 = 0;
const CONTINUATION_FRAME_TAG: u8 = 1;
const FIRST_FRAME_HEADER_LEN: usize = 1 + 8 + 4 + 4;
const CONTINUATION_FRAME_HEADER_LEN: usize = 1 + 8 + 4 + 1 + 4;

/// Parse the internal Hotline assembly payload emitted by `HotlineFrameCodec`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HotlineMessageAssembler;

impl HotlineMessageAssembler {
    /// Create a new assembler instance.
    #[must_use]
    pub const fn new() -> Self { Self }
}

impl MessageAssembler for HotlineMessageAssembler {
    fn parse_frame_header(&self, payload: &[u8]) -> Result<ParsedFrameHeader, io::Error> {
        let Some((&tag, _)) = payload.split_first() else {
            return Err(short_payload_error("missing Hotline assembly tag"));
        };

        match tag {
            FIRST_FRAME_TAG => parse_first_frame_header(payload),
            CONTINUATION_FRAME_TAG => parse_continuation_frame_header(payload),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown Hotline assembly frame tag: {tag}"),
            )),
        }
    }
}

/// Derive a stable message key for one logical Hotline transaction.
#[must_use]
pub(crate) fn message_key_for(header: &FrameHeader) -> MessageKey {
    let key = (u64::from(header.is_reply & 1) << 63)
        | (u64::from(header.ty) << 32)
        | u64::from(header.id);
    MessageKey(key)
}

/// Shared frame-payload builder used by [`first_frame_payload`] and
/// [`continuation_frame_payload`].
///
/// Layout: `tag(1) | message_key(8) | middle | body_len(4) | metadata | body`
#[expect(
    clippy::big_endian_bytes,
    reason = "internal Hotline transport metadata uses network byte order"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "frame assembly varies over explicit transport segments and error context"
)]
fn assemble_frame_payload(
    tag: u8,
    message_key: MessageKey,
    middle: &[u8],
    body: &[u8],
    metadata: &[u8],
    body_len_err: &'static str,
) -> Result<Vec<u8>, io::Error> {
    let body_len = u32::try_from(body.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, body_len_err))?;
    let capacity = 1 + 8 + middle.len() + 4 + metadata.len() + body.len();
    let mut payload = Vec::with_capacity(capacity);
    payload.push(tag);
    payload.extend_from_slice(&u64::from(message_key).to_be_bytes());
    payload.extend_from_slice(middle);
    payload.extend_from_slice(&body_len.to_be_bytes());
    payload.extend_from_slice(metadata);
    payload.extend_from_slice(body);
    Ok(payload)
}

/// Build the internal payload representation for the first physical fragment.
///
/// The metadata bytes are the normalized 20-byte logical transaction header
/// with `data_size == total_size`, so a completed assembly reconstructs the
/// existing `header || payload` shape consumed by `parse_transaction`.
#[expect(
    clippy::big_endian_bytes,
    reason = "internal Hotline transport metadata uses network byte order"
)]
pub(crate) fn first_frame_payload(
    message_key: MessageKey,
    header: &FrameHeader,
    body: &[u8],
) -> Result<Vec<u8>, io::Error> {
    assemble_frame_payload(
        FIRST_FRAME_TAG,
        message_key,
        &header.total_size.to_be_bytes(),
        body,
        &logical_header_bytes(header),
        "Hotline first-frame body length exceeds u32",
    )
}

/// Build the internal payload representation for a continuation fragment.
#[expect(
    clippy::big_endian_bytes,
    reason = "internal Hotline transport metadata uses network byte order"
)]
pub(crate) fn continuation_frame_payload(
    message_key: MessageKey,
    sequence: FrameSequence,
    is_last: bool,
    body: &[u8],
) -> Result<Vec<u8>, io::Error> {
    let mut middle = [0u8; 5];
    middle[..4].copy_from_slice(&u32::from(sequence).to_be_bytes());
    middle[4] = u8::from(is_last);
    assemble_frame_payload(
        CONTINUATION_FRAME_TAG,
        message_key,
        &middle,
        body,
        &[],
        "Hotline continuation body length exceeds u32",
    )
}

fn parse_first_frame_header(payload: &[u8]) -> Result<ParsedFrameHeader, io::Error> {
    if payload.len() < FIRST_FRAME_HEADER_LEN + HEADER_LEN {
        return Err(short_payload_error(
            "Hotline first-frame payload shorter than metadata header",
        ));
    }

    let message_key = MessageKey(read_u64(slice(
        payload,
        1,
        9,
        "missing first-frame message key",
    )?)?);
    let total_body_len = usize::try_from(read_u32(slice(
        payload,
        9,
        13,
        "missing first-frame total body length",
    )?)?)
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Hotline first-frame total length exceeds usize",
        )
    })?;
    let body_len = usize::try_from(read_u32(slice(
        payload,
        13,
        17,
        "missing first-frame body length",
    )?)?)
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Hotline first-frame body length exceeds usize",
        )
    })?;
    if body_len > total_body_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Hotline first-frame body length exceeds total length",
        ));
    }

    Ok(ParsedFrameHeader::new(
        AssemblyFrameHeader::First(FirstFrameHeader {
            message_key,
            metadata_len: HEADER_LEN,
            body_len,
            total_body_len: Some(total_body_len),
            is_last: body_len == total_body_len,
        }),
        FIRST_FRAME_HEADER_LEN,
    ))
}

fn parse_continuation_frame_header(payload: &[u8]) -> Result<ParsedFrameHeader, io::Error> {
    if payload.len() < CONTINUATION_FRAME_HEADER_LEN {
        return Err(short_payload_error(
            "Hotline continuation payload shorter than metadata header",
        ));
    }

    let message_key = MessageKey(read_u64(slice(
        payload,
        1,
        9,
        "missing continuation message key",
    )?)?);
    let sequence = FrameSequence(read_u32(slice(
        payload,
        9,
        13,
        "missing continuation sequence",
    )?)?);
    let is_last = match byte(payload, 13, "missing continuation last flag")? {
        0 => false,
        1 => true,
        value => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid Hotline continuation last-flag value: {value}"),
            ));
        }
    };
    let body_len = usize::try_from(read_u32(slice(
        payload,
        14,
        18,
        "missing continuation body length",
    )?)?)
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Hotline continuation body length exceeds usize",
        )
    })?;

    Ok(ParsedFrameHeader::new(
        AssemblyFrameHeader::Continuation(ContinuationFrameHeader {
            message_key,
            sequence: Some(sequence),
            body_len,
            is_last,
        }),
        CONTINUATION_FRAME_HEADER_LEN,
    ))
}

fn logical_header_bytes(header: &FrameHeader) -> [u8; HEADER_LEN] {
    let mut bytes = [0u8; HEADER_LEN];
    let mut logical = header.clone();
    logical.data_size = logical.total_size;
    logical.write_bytes(&mut bytes);
    bytes
}

fn slice<'a>(
    payload: &'a [u8],
    start: usize,
    end: usize,
    context: &'static str,
) -> Result<&'a [u8], io::Error> {
    payload
        .get(start..end)
        .ok_or_else(|| short_payload_error(context))
}

fn byte(payload: &[u8], index: usize, context: &'static str) -> Result<u8, io::Error> {
    payload
        .get(index)
        .copied()
        .ok_or_else(|| short_payload_error(context))
}

#[expect(
    clippy::big_endian_bytes,
    reason = "internal Hotline transport metadata uses network byte order"
)]
fn read_u32(bytes: &[u8]) -> Result<u32, io::Error> {
    let array: [u8; 4] = bytes
        .try_into()
        .map_err(|_| short_payload_error("expected 4 bytes"))?;
    Ok(u32::from_be_bytes(array))
}

#[expect(
    clippy::big_endian_bytes,
    reason = "internal Hotline transport metadata uses network byte order"
)]
fn read_u64(bytes: &[u8]) -> Result<u64, io::Error> {
    let array: [u8; 8] = bytes
        .try_into()
        .map_err(|_| short_payload_error("expected 8 bytes"))?;
    Ok(u64::from_be_bytes(array))
}

fn short_payload_error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, message)
}

#[cfg(test)]
mod tests {
    //! Unit coverage for Hotline message-assembly payload metadata.

    use wireframe::message_assembler::FrameHeader as AssemblyFrameHeader;

    use super::*;

    fn header(total_size: u32, data_size: u32) -> FrameHeader {
        FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 9,
            error: 0,
            total_size,
            data_size,
        }
    }

    #[test]
    fn message_key_includes_type_and_identifier() {
        let first = header(10, 5);
        let mut second = first.clone();
        second.ty = 108;

        assert_ne!(message_key_for(&first), message_key_for(&second));
    }

    #[test]
    fn first_frame_payload_reports_logical_header_metadata() {
        let header = header(10, 4);
        let payload =
            first_frame_payload(message_key_for(&header), &header, b"data").expect("payload");
        let parsed = HotlineMessageAssembler::new()
            .parse_frame_header(&payload)
            .expect("parsed header");

        match parsed.header() {
            AssemblyFrameHeader::First(first) => {
                assert_eq!(first.metadata_len, HEADER_LEN);
                assert_eq!(first.body_len, 4);
                assert_eq!(first.total_body_len, Some(10));
                assert!(!first.is_last);
            }
            other @ AssemblyFrameHeader::Continuation(_) => {
                panic!("expected first frame header, got {other:?}");
            }
        }

        let metadata = &payload[parsed.header_len()..parsed.header_len() + HEADER_LEN];
        let logical = FrameHeader::from_bytes(
            metadata
                .try_into()
                .expect("metadata stores a normalized 20-byte header"),
        );
        assert_eq!(logical.total_size, 10);
        assert_eq!(logical.data_size, 10);
    }

    #[test]
    fn continuation_payload_reports_sequence_and_last_flag() {
        let payload = continuation_frame_payload(MessageKey(11), FrameSequence(2), true, b"tail")
            .expect("payload");
        let parsed = HotlineMessageAssembler::new()
            .parse_frame_header(&payload)
            .expect("parsed header");

        match parsed.header() {
            AssemblyFrameHeader::Continuation(next) => {
                assert_eq!(next.message_key, MessageKey(11));
                assert_eq!(next.sequence, Some(FrameSequence(2)));
                assert_eq!(next.body_len, 4);
                assert!(next.is_last);
            }
            other @ AssemblyFrameHeader::First(_) => {
                panic!("expected continuation header, got {other:?}");
            }
        }
    }
}
