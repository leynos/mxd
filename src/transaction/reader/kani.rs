//! Kani harnesses for transaction reader framing invariants.

use super::{headers_match, validate_continuation_frame, validate_first_header};
use crate::transaction::kani_support::any_frame_header;

const KANI_MAX_TOTAL: u32 = 64;
const KANI_MAX_TOTAL_USIZE: usize = KANI_MAX_TOTAL as usize;

#[kani::proof]
fn kani_validate_first_header_matches_predicate() {
    let header = any_frame_header();

    let max_total_plus = KANI_MAX_TOTAL.saturating_add(1);
    kani::assume(header.total_size <= max_total_plus);
    kani::assume(header.data_size <= max_total_plus);

    let expected_ok = header.flags == 0
        && header.total_size <= KANI_MAX_TOTAL
        && header.data_size <= header.total_size
        && !(header.data_size == 0 && header.total_size > 0);

    kani::assert(
        validate_first_header(&header, KANI_MAX_TOTAL_USIZE).is_ok() == expected_ok,
        "first header validation matches predicate",
    );
}

#[kani::proof]
fn kani_validate_continuation_frame_matches_predicate() {
    let first = any_frame_header();
    let next = any_frame_header();
    let remaining: u32 = kani::any();

    let max_total_plus = KANI_MAX_TOTAL.saturating_add(1);
    kani::assume(remaining <= KANI_MAX_TOTAL);
    kani::assume(next.data_size <= max_total_plus);

    let expected_ok =
        headers_match(&first, &next) && next.data_size > 0 && next.data_size <= remaining;

    kani::assert(
        validate_continuation_frame(&first, &next, remaining).is_ok() == expected_ok,
        "continuation validation matches predicate",
    );
}
