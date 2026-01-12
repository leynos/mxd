//! Kani harnesses for Hotline transaction framing invariants.

use super::{for_each_fragment_range, validate_header};
use crate::transaction::{MAX_FRAME_DATA, MAX_PAYLOAD_SIZE, kani_support::any_frame_header};

const _: () = assert!(MAX_PAYLOAD_SIZE <= u32::MAX as usize);
const _: () = assert!(MAX_FRAME_DATA <= u32::MAX as usize);
const MAX_TOTAL_U32: u32 = MAX_PAYLOAD_SIZE as u32;
const MAX_DATA_U32: u32 = MAX_FRAME_DATA as u32;
const KANI_MAX_FRAGMENTS: usize = 2;
const KANI_MAX_PAYLOAD: usize = MAX_FRAME_DATA.saturating_mul(KANI_MAX_FRAGMENTS);

#[kani::proof]
fn kani_validate_header_matches_predicate() {
    let header = any_frame_header();

    let max_total_plus = MAX_TOTAL_U32.saturating_add(1);
    let max_data_plus = MAX_DATA_U32.saturating_add(1);
    kani::assume(header.total_size <= max_total_plus);
    kani::assume(header.data_size <= max_data_plus);

    let expected_ok = header.flags == 0
        && header.total_size <= MAX_TOTAL_U32
        && header.data_size <= MAX_DATA_U32
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
    let result = for_each_fragment_range(payload_len, |offset, len| {
        kani::assert(len > 0, "fragment has non-zero length");
        kani::assert(len <= MAX_FRAME_DATA, "fragment length within max frame");
        kani::assert(offset + len <= payload_len, "fragment within payload");
        sum += len;
        count += 1;
        Ok::<(), ()>(())
    });
    kani::assert(result.is_ok(), "fragment ranges iteration succeeds");

    kani::assert(sum == payload_len, "fragments cover payload exactly");
    if payload_len == 0 {
        kani::assert(count == 0, "zero payload yields zero fragments");
    } else {
        kani::assert(count > 0, "non-zero payload yields fragments");
    }
}
