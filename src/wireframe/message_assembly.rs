//! Hotline-specific protocol message assembly helpers for Wireframe.
//!
//! This adapter keeps Hotline fragment sequencing and logical transaction
//! reconstruction inside the transport layer. It exposes enough metadata for
//! Wireframe's message-assembly subsystem to enforce budgets while preserving
//! the downstream `header || payload` byte shape expected by MXD's routing
//! path.

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
/// This is a logical request budget, not a physical frame ceiling; the extra
/// headroom lets Wireframe size assembly against the full transaction envelope.
pub(crate) const HOTLINE_LOGICAL_MESSAGE_BYTES: usize = HEADER_LEN + MAX_PAYLOAD_SIZE;

const FIRST_FRAME_TAG: u8 = 0;
const CONTINUATION_FRAME_TAG: u8 = 1;
const FIRST_FRAME_HEADER_LEN: usize = 1 + 8 + 4 + 4;
const CONTINUATION_FRAME_HEADER_LEN: usize = 1 + 8 + 4 + 1 + 4;

/// Internal tag byte distinguishing first from continuation assembly payloads.
#[derive(Clone, Copy)]
enum AssemblyFrameTag {
    First,
    Continuation,
}

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

        match parse_frame_tag(tag)? {
            AssemblyFrameTag::First => parse_first_frame_header(payload),
            AssemblyFrameTag::Continuation => parse_continuation_frame_header(payload),
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

/// Frame-type-specific parameters for [`assemble_frame_payload`].
#[derive(Clone, Copy)]
struct FramePayloadSpec<'a> {
    /// Tag byte written at the start of the assembly payload.
    tag: u8,
    /// Bytes placed between `message_key` and `body_len`.
    middle: &'a [u8],
    /// Bytes written after `body_len` and before the body.
    metadata: &'a [u8],
    /// Error text surfaced when the body length exceeds `u32`.
    body_len_err: &'static str,
}

/// Shared frame-payload builder used by [`first_frame_payload`] and
/// [`continuation_frame_payload`].
///
/// Layout: `tag(1) | message_key(8) | middle | body_len(4) | metadata | body`
#[expect(
    clippy::big_endian_bytes,
    reason = "internal Hotline transport metadata uses network byte order"
)]
fn assemble_frame_payload(
    spec: FramePayloadSpec<'_>,
    message_key: MessageKey,
    body: &[u8],
) -> Result<Vec<u8>, io::Error> {
    let body_len = u32::try_from(body.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, spec.body_len_err))?;
    let capacity = 1 + 8 + spec.middle.len() + 4 + spec.metadata.len() + body.len();
    let mut payload = Vec::with_capacity(capacity);
    payload.push(spec.tag);
    payload.extend_from_slice(&u64::from(message_key).to_be_bytes());
    payload.extend_from_slice(spec.middle);
    payload.extend_from_slice(&body_len.to_be_bytes());
    payload.extend_from_slice(spec.metadata);
    payload.extend_from_slice(body);
    Ok(payload)
}

/// Build the internal payload for the first physical fragment.
/// The metadata stores a normalized 20-byte logical header with
/// `data_size == total_size`, preserving the `header || payload` shape that
/// `parse_transaction` consumes after assembly.
#[expect(
    clippy::big_endian_bytes,
    reason = "internal Hotline transport metadata uses network byte order"
)]
pub(crate) fn first_frame_payload(
    message_key: MessageKey,
    header: &FrameHeader,
    body: &[u8],
) -> Result<Vec<u8>, io::Error> {
    let total_size_bytes = header.total_size.to_be_bytes();
    let metadata = logical_header_bytes(header);
    assemble_frame_payload(
        FramePayloadSpec {
            tag: FIRST_FRAME_TAG,
            middle: &total_size_bytes,
            metadata: &metadata,
            body_len_err: "Hotline first-frame body length exceeds u32",
        },
        message_key,
        body,
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
        FramePayloadSpec {
            tag: CONTINUATION_FRAME_TAG,
            middle: &middle,
            metadata: &[],
            body_len_err: "Hotline continuation body length exceeds u32",
        },
        message_key,
        body,
    )
}

/// Parse the internal first-fragment assembly payload into a `ParsedFrameHeader`.
fn parse_first_frame_header(payload: &[u8]) -> Result<ParsedFrameHeader, io::Error> {
    if payload.len() < FIRST_FRAME_HEADER_LEN + HEADER_LEN {
        return Err(short_payload_error(
            "Hotline first-frame payload shorter than metadata header",
        ));
    }

    let mut cursor = PayloadCursor::new(payload);
    let tag = cursor.u8("missing Hotline assembly tag")?;
    debug_assert!(matches!(parse_frame_tag(tag), Ok(AssemblyFrameTag::First)));
    let message_key = MessageKey(cursor.u64("missing first-frame message key")?);
    let total_body_len = u32_to_usize(
        cursor.u32("missing first-frame total body length")?,
        "Hotline first-frame total length exceeds usize",
    )?;
    let body_len = u32_to_usize(
        cursor.u32("missing first-frame body length")?,
        "Hotline first-frame body length exceeds usize",
    )?;
    if body_len > total_body_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Hotline first-frame body length exceeds total length",
        ));
    }
    let expected_len = FIRST_FRAME_HEADER_LEN + HEADER_LEN + body_len;
    if payload.len() != expected_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Hotline first-frame payload length does not match declared body length",
        ));
    }
    let logical_header = parse_logical_header(payload)?;
    let logical_total_len = u32_to_usize(
        logical_header.total_size,
        "Hotline logical first-frame total length exceeds usize",
    )?;
    if logical_total_len != total_body_len || logical_header.data_size != logical_header.total_size
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Hotline first-frame logical header length does not match declared total length",
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

/// Parse the internal continuation-fragment assembly payload into a `ParsedFrameHeader`.
fn parse_continuation_frame_header(payload: &[u8]) -> Result<ParsedFrameHeader, io::Error> {
    if payload.len() < CONTINUATION_FRAME_HEADER_LEN {
        return Err(short_payload_error(
            "Hotline continuation payload shorter than metadata header",
        ));
    }

    let mut cursor = PayloadCursor::new(payload);
    let tag = cursor.u8("missing Hotline assembly tag")?;
    debug_assert!(matches!(
        parse_frame_tag(tag),
        Ok(AssemblyFrameTag::Continuation)
    ));
    let message_key = MessageKey(cursor.u64("missing continuation message key")?);
    let sequence = FrameSequence(cursor.u32("missing continuation sequence")?);
    let is_last = parse_last_flag(cursor.u8("missing continuation last flag")?)?;
    let body_len = u32_to_usize(
        cursor.u32("missing continuation body length")?,
        "Hotline continuation body length exceeds usize",
    )?;
    let expected_len = CONTINUATION_FRAME_HEADER_LEN + body_len;
    if payload.len() != expected_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Hotline continuation payload length does not match declared body length",
        ));
    }

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

/// Serialize a normalized copy of `header` with `data_size == total_size`.
fn logical_header_bytes(header: &FrameHeader) -> [u8; HEADER_LEN] {
    let mut bytes = [0u8; HEADER_LEN];
    let mut logical = header.clone();
    logical.data_size = logical.total_size;
    logical.write_bytes(&mut bytes);
    bytes
}

/// Parse the normalized logical header embedded in a first-frame payload.
fn parse_logical_header(payload: &[u8]) -> Result<FrameHeader, io::Error> {
    let metadata_start = FIRST_FRAME_HEADER_LEN;
    let metadata_end = metadata_start + HEADER_LEN;
    let metadata_bytes = payload
        .get(metadata_start..metadata_end)
        .ok_or_else(|| short_payload_error("missing Hotline logical metadata header"))?;
    let metadata: [u8; HEADER_LEN] = metadata_bytes
        .try_into()
        .map_err(|_| short_payload_error("missing Hotline logical metadata header"))?;
    Ok(FrameHeader::from_bytes(&metadata))
}

/// Map a raw tag byte to `AssemblyFrameTag`, rejecting unknown values.
fn parse_frame_tag(tag: u8) -> Result<AssemblyFrameTag, io::Error> {
    if tag == FIRST_FRAME_TAG {
        Ok(AssemblyFrameTag::First)
    } else if tag == CONTINUATION_FRAME_TAG {
        Ok(AssemblyFrameTag::Continuation)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown Hotline assembly frame tag: {tag}"),
        ))
    }
}

/// Interpret a raw last-flag byte as `bool`, rejecting values other than `0` or `1`.
fn parse_last_flag(flag: u8) -> Result<bool, io::Error> {
    if flag == 0 {
        Ok(false)
    } else if flag == 1 {
        Ok(true)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid Hotline continuation last-flag value: {flag}"),
        ))
    }
}

/// Convert a `u32` to `usize`, mapping overflow to an `InvalidData` error.
fn u32_to_usize(value: u32, err: &'static str) -> Result<usize, io::Error> {
    usize::try_from(value).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, err))
}

/// Cursor over an assembly payload slice that advances its position on each read.
struct PayloadCursor<'a> {
    payload: &'a [u8],
    pos: usize,
}

impl<'a> PayloadCursor<'a> {
    /// Create a new cursor at position 0.
    const fn new(payload: &'a [u8]) -> Self { Self { payload, pos: 0 } }

    /// Return `len` bytes at the current position and advance the cursor.
    fn take(&mut self, len: usize, context: &'static str) -> Result<&'a [u8], io::Error> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| short_payload_error(context))?;
        let bytes = self
            .payload
            .get(self.pos..end)
            .ok_or_else(|| short_payload_error(context))?;
        self.pos = end;
        Ok(bytes)
    }

    /// Read one byte and advance the cursor.
    fn u8(&mut self, context: &'static str) -> Result<u8, io::Error> {
        let bytes = self.take(1, context)?;
        bytes
            .first()
            .copied()
            .ok_or_else(|| short_payload_error(context))
    }

    /// Read a big-endian `u32` and advance the cursor.
    #[expect(
        clippy::big_endian_bytes,
        reason = "internal Hotline transport metadata uses network byte order"
    )]
    fn u32(&mut self, context: &'static str) -> Result<u32, io::Error> {
        let bytes = self.take(4, context)?;
        let array: [u8; 4] = bytes.try_into().map_err(|_| short_payload_error(context))?;
        Ok(u32::from_be_bytes(array))
    }

    /// Read a big-endian `u64` and advance the cursor.
    #[expect(
        clippy::big_endian_bytes,
        reason = "internal Hotline transport metadata uses network byte order"
    )]
    fn u64(&mut self, context: &'static str) -> Result<u64, io::Error> {
        let bytes = self.take(8, context)?;
        let array: [u8; 8] = bytes.try_into().map_err(|_| short_payload_error(context))?;
        Ok(u64::from_be_bytes(array))
    }
}

/// Construct an `UnexpectedEof` error for a truncated assembly payload.
fn short_payload_error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, message)
}
