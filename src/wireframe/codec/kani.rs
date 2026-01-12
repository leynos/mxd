//! Kani harnesses for Hotline transaction framing invariants.

use super::{fragment_ranges, validate_header};
use crate::transaction::{FrameHeader, MAX_FRAME_DATA, MAX_PAYLOAD_SIZE};

const KANI_MAX_FRAGMENTS: usize = 2;
const KANI_MAX_PAYLOAD: usize = MAX_FRAME_DATA.saturating_mul(KANI_MAX_FRAGMENTS);

fn any_header() -> FrameHeader {
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

#[kani::proof]
fn kani_validate_header_matches_predicate() {
    let header = any_header();

    let max_total = match u32::try_from(MAX_PAYLOAD_SIZE) {
        Ok(value) => value,
        Err(_) => return,
    };
    let max_data = match u32::try_from(MAX_FRAME_DATA) {
        Ok(value) => value,
        Err(_) => return,
    };

    let max_total_plus = max_total.saturating_add(1);
    let max_data_plus = max_data.saturating_add(1);
    kani::assume(header.total_size <= max_total_plus);
    kani::assume(header.data_size <= max_data_plus);

    let expected_ok = header.flags == 0
        && header.total_size <= max_total
        && header.data_size <= max_data
        && header.data_size <= header.total_size
        && !(header.data_size == 0 && header.total_size > 0);

    kani::assert(
        validate_header(&header).is_ok() == expected_ok,
        "header validation matches predicate",
    );
}

#[kani::proof]
#[kani::unwind(3)]
fn kani_fragment_ranges_cover_payload() {
    let max_total = MAX_PAYLOAD_SIZE.min(KANI_MAX_PAYLOAD);
    let payload_len: u16 = kani::any();
    kani::assume(usize::from(payload_len) <= max_total);

    let payload_len = usize::from(payload_len);

    let mut sum = 0usize;
    let mut count = 0usize;
    for (offset, len) in fragment_ranges(payload_len) {
        kani::assert(len > 0, "fragment has non-zero length");
        kani::assert(len <= MAX_FRAME_DATA, "fragment length within max frame");
        kani::assert(offset + len <= payload_len, "fragment within payload");
        sum += len;
        count += 1;
    }

    kani::assert(sum == payload_len, "fragments cover payload exactly");
    if payload_len == 0 {
        kani::assert(count == 0, "zero payload yields zero fragments");
    } else {
        kani::assert(count > 0, "non-zero payload yields fragments");
    }
}
