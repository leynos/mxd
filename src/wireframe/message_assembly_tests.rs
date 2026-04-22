//! Regression tests for Hotline message-assembly payload validation.

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

#[test]
fn first_frame_parser_rejects_declared_body_length_mismatches() {
    let header = header(10, 4);
    let mut payload =
        first_frame_payload(message_key_for(&header), &header, b"data").expect("payload");
    payload[13..17].copy_from_slice(&5u32.to_be_bytes());

    let err = HotlineMessageAssembler::new()
        .parse_frame_header(&payload)
        .expect_err("declared first-frame body length must match trailing bytes");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string()
            .contains("Hotline first-frame payload length does not match declared body length"),
        "unexpected error: {err}"
    );
}

#[test]
fn continuation_parser_rejects_declared_body_length_mismatches() {
    let mut payload = continuation_frame_payload(MessageKey(11), FrameSequence(2), true, b"tail")
        .expect("payload");
    payload[14..18].copy_from_slice(&5u32.to_be_bytes());

    let err = HotlineMessageAssembler::new()
        .parse_frame_header(&payload)
        .expect_err("declared continuation body length must match trailing bytes");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string()
            .contains("Hotline continuation payload length does not match declared body length",),
        "unexpected error: {err}"
    );
}

#[test]
fn first_frame_parser_rejects_logical_header_total_mismatches() {
    let header = header(10, 4);
    let mut payload =
        first_frame_payload(message_key_for(&header), &header, b"data").expect("payload");
    payload[29..33].copy_from_slice(&9u32.to_be_bytes());

    let err = HotlineMessageAssembler::new()
        .parse_frame_header(&payload)
        .expect_err("logical header total must match declared first-frame total");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains(
            "Hotline first-frame logical header length does not match declared total length",
        ),
        "unexpected error: {err}"
    );
}
