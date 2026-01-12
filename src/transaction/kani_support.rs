//! Shared Kani helpers for transaction framing proofs.

use super::FrameHeader;

/// Create an arbitrary `FrameHeader` for Kani harnesses.
///
/// # Examples
///
/// ```rust,ignore
/// use crate::transaction::kani_support::any_frame_header;
///
/// let header = any_frame_header();
/// kani::assume(header.total_size <= 64);
/// ```
pub fn any_frame_header() -> FrameHeader {
    FrameHeader {
        flags: kani::any(),
        is_reply: kani::any(),
        ty: kani::any(),
        id: kani::any(),
        error: kani::any(),
        total_size: kani::any(),
        data_size: kani::any(),
    }
}
