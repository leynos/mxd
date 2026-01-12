//! Shared Kani helpers for transaction framing proofs.

use super::FrameHeader;

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
