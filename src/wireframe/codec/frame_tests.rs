//! Tests cover `HotlineFrameCodec` payload wrapping, inbound fragment
//! metadata, and logical message-budget invariants.

use bytes::{Bytes, BytesMut};
use rstest::{fixture, rstest};
use tokio_util::codec::Decoder as _;
use wireframe::{
    app::{Envelope, Packet},
    codec::FrameCodec,
    correlation::CorrelatableFrame,
    message::Message,
};

use super::HotlineFrameCodec;
use crate::{
    transaction::{FrameHeader, HEADER_LEN, MAX_PAYLOAD_SIZE},
    wireframe::test_helpers::fragmented_transaction_bytes,
};

#[fixture]
fn codec() -> HotlineFrameCodec {
    // Provide a fresh codec instance per rstest case.
    HotlineFrameCodec::new()
}

#[rstest]
#[case(Bytes::from(vec![0u8, 1u8, 2u8, 3u8, 4u8]), vec![0u8, 1u8, 2u8, 3u8, 4u8])]
#[case(Bytes::new(), Vec::new())]
fn wrap_payload_cases(codec: HotlineFrameCodec, #[case] bytes: Bytes, #[case] expected: Vec<u8>) {
    let frame = codec.wrap_payload(bytes);

    assert_eq!(frame, expected);
}

#[rstest]
#[case(vec![10u8, 20u8, 30u8], vec![10u8, 20u8, 30u8])]
#[case(Vec::new(), Vec::new())]
fn frame_payload_cases(#[case] data: Vec<u8>, #[case] expected: Vec<u8>) {
    let slice = HotlineFrameCodec::frame_payload(&data);

    assert_eq!(slice, expected.as_slice());
}

#[test]
fn max_frame_length_matches_logical_message_budget() {
    let codec = HotlineFrameCodec::new();

    assert_eq!(codec.max_frame_length(), HEADER_LEN + MAX_PAYLOAD_SIZE);
}

#[test]
fn codec_round_trip_payload_unchanged() {
    let codec = HotlineFrameCodec::new();
    let original = vec![0xabu8, 0xcdu8, 0xefu8];
    let bytes = Bytes::from(original.clone());
    let frame = codec.wrap_payload(bytes);
    let extracted = HotlineFrameCodec::frame_payload(&frame);
    assert_eq!(extracted, original);
}

#[test]
fn decoder_emits_first_then_continuation_payloads_for_fragmented_request() {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 44,
        error: 0,
        total_size: 6,
        data_size: 6,
    };
    let fragments = fragmented_transaction_bytes(&header, b"abcdef", 4).expect("fragments");
    let mut bytes = BytesMut::new();
    let mut decoder = HotlineFrameCodec::new().decoder();

    bytes.extend_from_slice(&fragments[0]);
    let first = decoder
        .decode(&mut bytes)
        .expect("decode first frame")
        .expect("first frame payload");
    let (first_env, _) = Envelope::from_bytes(&first).expect("decode first envelope");
    assert_eq!(
        first_env.id(),
        crate::wireframe::route_ids::route_id_for(107)
    );
    assert_eq!(first_env.correlation_id(), Some(44));

    bytes.extend_from_slice(&fragments[1]);
    let second = decoder
        .decode(&mut bytes)
        .expect("decode continuation")
        .expect("continuation payload");
    let (second_env, _) = Envelope::from_bytes(&second).expect("decode second envelope");
    assert_eq!(
        second_env.id(),
        crate::wireframe::route_ids::route_id_for(107)
    );
    assert_eq!(second_env.correlation_id(), Some(44));
}
fn tracker_with_pending_series() -> (super::InboundSeriesTracker, FrameHeader, FrameHeader) {
    let first_header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 55,
        error: 0,
        total_size: 4,
        data_size: 2,
    };
    let mut tracker = super::InboundSeriesTracker::new();
    if let Err(err) = tracker.start(&first_header, &[0u8; 2]) {
        panic!("first fragment starts a tracked series: {err}");
    }

    let zero_header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 55,
        error: 0,
        total_size: 4,
        data_size: 0,
    };

    (tracker, first_header, zero_header)
}

#[test]
fn decoder_rejects_zero_byte_continuation_fragment() {
    let (mut tracker, _, zero_header) = tracker_with_pending_series();
    let err = tracker
        .continue_series(&zero_header, &[])
        .expect_err("zero-byte continuation must be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("no progress"),
        "unexpected error message: {err}"
    );
}

#[test]
fn continue_series_clears_active_state_on_zero_progress_fragment() {
    let (mut tracker, _, zero_header) = tracker_with_pending_series();
    let err = tracker
        .continue_series(&zero_header, b"")
        .expect_err("zero-progress continuation must be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("no progress"),
        "unexpected error message: {err}"
    );
    assert!(
        !tracker.has_active_series(),
        "zero-progress continuation must clear the active series"
    );
}
