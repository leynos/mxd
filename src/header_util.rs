//! Helpers for working with frame headers.
//!
//! Provides utility functions for building reply [`crate::transaction::FrameHeader`] values that
//! mirror a request. These helpers keep the framing logic centralized and
//! consistent across commands.

/// Build a reply `FrameHeader` mirroring the request and specifying
/// the payload size and error code.
///
/// # Panics
/// Panics if `payload_len` does not fit within `u32`.
#[must_use]
pub fn reply_header(
    req: &crate::transaction::FrameHeader,
    payload_error: u32,
    payload_len: usize,
) -> crate::transaction::FrameHeader {
    let Ok(payload_len_u32) = u32::try_from(payload_len) else {
        panic!("payload length exceeds u32::MAX: {payload_len}");
    };
    crate::transaction::FrameHeader {
        flags: 0,
        is_reply: 1,
        ty: req.ty,
        id: req.id,
        error: payload_error,
        total_size: payload_len_u32,
        data_size: payload_len_u32,
    }
}

#[cfg(kani)]
mod kani;
