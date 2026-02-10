//! Kani harnesses for XOR compatibility invariants.

use super::{xor_bytes, xor_params};
use crate::field_id::FieldId;

#[kani::proof]
fn kani_xor_bytes_round_trip_bounded() {
    let input: [u8; 8] = kani::any();

    let encoded = xor_bytes(&input);
    let decoded = xor_bytes(&encoded);

    kani::assert(
        decoded.as_slice() == input.as_slice(),
        "XOR byte transform is involutive",
    );
}

#[kani::proof]
fn kani_xor_payload_round_trip_text_fields_bounded() {
    let text_bytes: [u8; 4] = kani::any();
    let numeric_bytes: [u8; 4] = kani::any();
    let original_params = vec![
        (FieldId::Data, text_bytes.to_vec()),
        (FieldId::NewsArticleId, numeric_bytes.to_vec()),
    ];
    let encoded_params = xor_params(&original_params);
    let decoded_params = xor_params(&encoded_params);

    kani::assert(
        decoded_params == original_params,
        "XOR compatibility round-trips parameter vectors",
    );

    let original_numeric = original_params
        .iter()
        .find(|(field, _)| *field == FieldId::NewsArticleId)
        .map(|(_, data)| data.as_slice());
    let encoded_numeric = encoded_params
        .iter()
        .find(|(field, _)| *field == FieldId::NewsArticleId)
        .map(|(_, data)| data.as_slice());

    kani::assert(
        original_numeric == encoded_numeric,
        "non-text field bytes are unchanged by XOR compatibility encoding",
    );
}
