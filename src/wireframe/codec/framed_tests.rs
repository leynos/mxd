//! Tests for the Hotline Tokio codec.

use rstest::rstest;

use super::*;
use crate::{
    field_id::FieldId,
    wireframe::test_helpers::{fragmented_transaction_bytes, transaction_bytes},
};

fn prepare_reassembly_buffer(
    codec: &mut HotlineCodec,
    header: &FrameHeader,
    first_payload: &[u8],
    second_payload: &[u8],
) -> BytesMut {
    let first = transaction_bytes(header, first_payload);
    let mut buf = BytesMut::from(&first[..]);

    let first_result = codec.decode(&mut buf).expect("decode should succeed");

    assert!(first_result.is_none());

    let mut second_header = header.clone();
    second_header.data_size =
        u32::try_from(second_payload.len()).expect("second payload length should fit in u32");
    let second = transaction_bytes(&second_header, second_payload);
    buf.extend_from_slice(&second);
    buf
}

#[rstest]
#[case::single_frame(
    HotlineTransaction::request_from_params(
        107,
        1,
        &[
            (FieldId::Login, b"alice".as_slice()),
            (FieldId::Password, b"secret".as_slice()),
        ],
    )
    .expect("request tx"),
    107,
    0,
)]
#[case::empty_frame(
    HotlineTransaction::reply_from_params(
        &FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 200,
            id: 42,
            error: 0,
            total_size: 0,
            data_size: 0,
        },
        0,
        &[] as &[(FieldId, &[u8])],
    )
    .expect("reply tx"),
    200,
    1,
)]
fn decodes_frame(
    #[case] tx: HotlineTransaction,
    #[case] expected_ty: u16,
    #[case] expected_is_reply: u8,
) {
    let mut codec = HotlineCodec::new();
    let (header, payload) = tx.into_parts();
    let mut buf = BytesMut::from(&transaction_bytes(&header, &payload)[..]);

    let result = codec.decode(&mut buf).expect("decode should succeed");

    let decoded = result.expect("should produce transaction");
    assert_eq!(decoded.header().ty, expected_ty);
    assert_eq!(decoded.header().is_reply, expected_is_reply);
    assert_eq!(decoded.payload(), payload.as_slice());
}

#[rstest]
fn returns_none_for_partial_header() {
    let mut codec = HotlineCodec::new();
    let mut buf = BytesMut::from(&[0u8; 10][..]);

    let result = codec.decode(&mut buf).expect("decode should succeed");

    assert!(result.is_none());
    // Buffer should be unchanged
    assert_eq!(buf.len(), 10);
}

#[rstest]
fn returns_none_for_partial_payload() {
    let mut codec = HotlineCodec::new();
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 100,
        data_size: 100,
    };
    // Only include header + 10 bytes of payload
    let bytes = transaction_bytes(&header, &[0u8; 10]);
    let mut buf = BytesMut::from(&bytes[..]);

    let result = codec.decode(&mut buf).expect("decode should succeed");

    assert!(result.is_none());
}

#[rstest]
fn encodes_single_frame() {
    let mut codec = HotlineCodec::new();
    let tx = HotlineTransaction::request_from_params(107, 99, &[(FieldId::Login, b"alice")])
        .expect("transaction");
    let payload = tx.payload().to_vec();
    let mut buf = BytesMut::new();

    codec.encode(tx, &mut buf).expect("encode should succeed");

    assert_eq!(buf.len(), HEADER_LEN + payload.len());
    let decoded_header = FrameHeader::from_bytes(
        buf[..HEADER_LEN]
            .try_into()
            .expect("header slice correct size"),
    );
    assert_eq!(decoded_header.ty, 107);
    assert_eq!(&buf[HEADER_LEN..], payload.as_slice());
}

#[rstest]
fn encodes_empty_frame() {
    let mut codec = HotlineCodec::new();
    let tx = HotlineTransaction::from_parts(
        FrameHeader {
            flags: 0,
            is_reply: 1,
            ty: 200,
            id: 1,
            error: 0,
            total_size: 0,
            data_size: 0,
        },
        Vec::new(),
    )
    .expect("transaction");
    let mut buf = BytesMut::new();

    codec.encode(tx, &mut buf).expect("encode should succeed");

    assert_eq!(buf.len(), HEADER_LEN);
}

#[rstest]
fn rejects_invalid_flags() {
    let mut codec = HotlineCodec::new();
    let header = FrameHeader {
        flags: 1,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    let mut buf = BytesMut::from(&transaction_bytes(&header, &[])[..]);

    let err = codec.decode(&mut buf).expect_err("decode should fail");

    assert!(err.to_string().contains("invalid flags"));
}

#[rstest]
fn reassembles_two_fragments() {
    let mut codec = HotlineCodec::new();
    let tx =
        HotlineTransaction::request_from_params(107, 7, &[(FieldId::Login, b"alice".as_slice())])
            .expect("transaction");
    let (header, payload) = tx.into_parts();
    let fragments = fragmented_transaction_bytes(&header, &payload, 6).expect("fragments");
    let mut buf = BytesMut::from(&fragments[0][..]);

    let first = codec.decode(&mut buf).expect("decode should succeed");

    assert!(first.is_none());
    buf.extend_from_slice(&fragments[1]);
    let decoded = codec
        .decode(&mut buf)
        .expect("decode should succeed")
        .expect("transaction");
    assert_eq!(decoded.payload(), payload.as_slice());
}

#[rstest]
fn rejects_fragment_exceeding_remaining() {
    let mut codec = HotlineCodec::new();
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 10,
        error: 0,
        total_size: 10,
        data_size: 6,
    };
    let mut buf = prepare_reassembly_buffer(&mut codec, &header, &[0u8; 6], &[0u8; 6]);

    let err = codec.decode(&mut buf).expect_err("decode should fail");

    assert!(
        err.to_string()
            .contains("fragment exceeds remaining payload size")
    );
}

#[rstest]
fn rejects_incomplete_reassembly_at_eof() {
    let mut codec = HotlineCodec::new();
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 11,
        error: 0,
        total_size: 10,
        data_size: 4,
    };
    let mut buf = prepare_reassembly_buffer(&mut codec, &header, &[0u8; 4], &[0u8; 4]);

    let second_result = codec.decode(&mut buf).expect("decode should succeed");
    assert!(second_result.is_none());
    let err = codec.decode_eof(&mut buf).expect_err("decode should fail");

    assert!(err.to_string().contains("incomplete transaction frame"));
}
