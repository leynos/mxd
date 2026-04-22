//! Regression tests for Hotline message-assembly payload validation.

use rstest::rstest;
use wireframe::message_assembler::{
    FrameHeader as AssemblyFrameHeader,
    FrameSequence,
    MessageAssembler,
    MessageKey,
};

use super::message_assembly::{
    HotlineMessageAssembler,
    continuation_frame_payload,
    first_frame_payload,
    message_key_for,
};
use crate::transaction::{FrameHeader, HEADER_LEN};

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
    let payload = first_frame_payload(message_key_for(&header), &header, b"data").expect("payload");
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

/// Corrupt one `u32` field in `payload` and assert that `parse_frame_header`
/// returns an `InvalidData` error whose message contains `expected_msg`.
fn assert_tampered_payload_rejected(
    mut payload: Vec<u8>,
    tamper_range: std::ops::Range<usize>,
    tampered_value: u32,
    expected_msg: &str,
) {
    payload[tamper_range].copy_from_slice(&tampered_value.to_be_bytes());
    let err = HotlineMessageAssembler::new()
        .parse_frame_header(&payload)
        .expect_err("parse_frame_header must return an error for tampered payload");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains(expected_msg),
        "unexpected error: {err}"
    );
}

#[derive(Clone, Copy)]
enum PayloadBuilder {
    First,
    Continuation,
    FirstLogicalHeaderTotal,
}

fn build_payload(builder: PayloadBuilder) -> Vec<u8> {
    match builder {
        PayloadBuilder::First | PayloadBuilder::FirstLogicalHeaderTotal => {
            let header = header(10, 4);
            first_frame_payload(message_key_for(&header), &header, b"data").expect("payload")
        }
        PayloadBuilder::Continuation => {
            continuation_frame_payload(MessageKey(11), FrameSequence(2), true, b"tail")
                .expect("payload")
        }
    }
}

#[rstest]
#[case(
    PayloadBuilder::First,
    13..17,
    5,
    "Hotline first-frame payload length does not match declared body length"
)]
#[case(
    PayloadBuilder::Continuation,
    14..18,
    5,
    "Hotline continuation payload length does not match declared body length"
)]
#[case(
    PayloadBuilder::FirstLogicalHeaderTotal,
    29..33,
    9,
    "Hotline first-frame logical header length does not match declared total length"
)]
fn tampered_payloads_are_rejected(
    #[case] builder: PayloadBuilder,
    #[case] tamper_range: std::ops::Range<usize>,
    #[case] tampered_value: u32,
    #[case] expected_msg: &str,
) {
    let payload = build_payload(builder);
    assert_tampered_payload_rejected(payload, tamper_range, tampered_value, expected_msg);
}
