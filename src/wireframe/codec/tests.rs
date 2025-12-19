//! Unit tests for the Wireframe transaction codec.

use std::io::Cursor;

use bincode::{config, encode_to_vec};
use rstest::rstest;
use tokio::io::BufReader;
use wireframe::preamble::read_preamble;

use super::*;
use crate::wireframe::test_helpers::transaction_bytes;

fn hotline_config() -> impl bincode::config::Config {
    config::standard()
        .with_big_endian()
        .with_fixed_int_encoding()
}

#[rstest]
#[case(20, 20)] // Single frame with payload
#[case(0, 0)] // Empty payload
#[tokio::test]
async fn decodes_valid_single_frame(#[case] total: u32, #[case] data: u32) {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: total,
        data_size: data,
    };
    let payload = vec![0u8; total as usize];
    let bytes = transaction_bytes(&header, &payload);
    let mut reader = BufReader::new(Cursor::new(bytes));

    let (tx, leftover) = read_preamble::<_, HotlineTransaction>(&mut reader)
        .await
        .expect("transaction must decode");

    assert!(leftover.is_empty());
    assert_eq!(tx.header().total_size, total);
    assert_eq!(tx.payload().len(), total as usize);
}

#[rstest]
#[case(10, 20, "data size exceeds total")]
#[case(100, 0, "data size is zero but total size is non-zero")]
#[tokio::test]
async fn rejects_invalid_length_combinations(
    #[case] total: u32,
    #[case] data: u32,
    #[case] expected_msg: &str,
) {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: total,
        data_size: data,
    };
    let payload = vec![0u8; data as usize];
    let bytes = transaction_bytes(&header, &payload);
    let mut reader = BufReader::new(Cursor::new(bytes));

    let err = read_preamble::<_, HotlineTransaction>(&mut reader)
        .await
        .expect_err("decode must fail");

    assert!(
        err.to_string().contains(expected_msg),
        "expected '{expected_msg}' in '{err}'"
    );
}

#[tokio::test]
async fn rejects_invalid_flags() {
    let header = FrameHeader {
        flags: 1,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    let bytes = transaction_bytes(&header, &[]);
    let mut reader = BufReader::new(Cursor::new(bytes));

    let err = read_preamble::<_, HotlineTransaction>(&mut reader)
        .await
        .expect_err("decode must fail");

    assert!(
        err.to_string().contains("invalid flags"),
        "expected 'invalid flags' in '{err}'"
    );
}

#[tokio::test]
async fn rejects_oversized_total() {
    let oversized_total = u32::try_from(MAX_PAYLOAD_SIZE + 1).expect("test size fits in u32");
    let frame_data = u32::try_from(MAX_FRAME_DATA).expect("frame data fits in u32");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: oversized_total,
        data_size: frame_data,
    };
    let payload = vec![0u8; MAX_FRAME_DATA];
    let bytes = transaction_bytes(&header, &payload);
    let mut reader = BufReader::new(Cursor::new(bytes));

    let err = read_preamble::<_, HotlineTransaction>(&mut reader)
        .await
        .expect_err("decode must fail");

    assert!(
        err.to_string().contains("total size exceeds maximum"),
        "expected 'total size exceeds maximum' in '{err}'"
    );
}

#[tokio::test]
async fn rejects_oversized_data() {
    let oversized = u32::try_from(MAX_FRAME_DATA + 1).expect("test size fits in u32");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: oversized,
        data_size: oversized,
    };
    let payload = vec![0u8; MAX_FRAME_DATA + 1];
    let bytes = transaction_bytes(&header, &payload);
    let mut reader = BufReader::new(Cursor::new(bytes));

    let err = read_preamble::<_, HotlineTransaction>(&mut reader)
        .await
        .expect_err("decode must fail");

    assert!(
        err.to_string().contains("data size exceeds maximum"),
        "expected 'data size exceeds maximum' in '{err}'"
    );
}

#[rstest]
#[case(0)]
#[case(MAX_FRAME_DATA)]
#[case(MAX_FRAME_DATA + 1)]
fn encodes_payloads_with_legacy_framing(#[case] payload_len: usize) {
    let payload = vec![0u8; payload_len];
    let header = FrameHeader {
        flags: 0,
        is_reply: 1,
        ty: 200,
        id: 42,
        error: 0,
        total_size: u32::try_from(payload_len).expect("len fits u32"),
        data_size: u32::try_from(payload_len).expect("len fits u32"),
    };

    let tx = HotlineTransaction {
        header: header.clone(),
        payload: payload.clone(),
    };

    let bytes = encode_to_vec(&tx, hotline_config()).expect("encode");

    let expected = if payload.is_empty() {
        transaction_bytes(
            &FrameHeader {
                data_size: 0,
                ..header
            },
            &[],
        )
    } else if payload.len() <= MAX_FRAME_DATA {
        transaction_bytes(
            &FrameHeader {
                data_size: u32::try_from(payload.len()).expect("len fits u32"),
                ..header
            },
            &payload,
        )
    } else {
        let first_chunk = payload
            .get(..MAX_FRAME_DATA)
            .expect("payload length checked before slicing");
        let second_chunk = payload
            .get(MAX_FRAME_DATA..)
            .expect("payload length checked before slicing");
        let first = transaction_bytes(
            &FrameHeader {
                data_size: u32::try_from(MAX_FRAME_DATA).expect("len fits u32"),
                ..header.clone()
            },
            first_chunk,
        );
        let rest = transaction_bytes(
            &FrameHeader {
                data_size: u32::try_from(payload.len() - MAX_FRAME_DATA).expect("len fits u32"),
                ..header
            },
            second_chunk,
        );
        [first, rest].concat()
    };

    assert_eq!(bytes, expected);
}

#[tokio::test]
async fn encoding_rejects_size_mismatch() {
    let tx = HotlineTransaction {
        header: FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size: 2,
            data_size: 2,
        },
        payload: Vec::new(),
    };

    let err = encode_to_vec(&tx, hotline_config()).expect_err("encode must fail");
    assert!(
        err.to_string().contains("size mismatch"),
        "expected 'size mismatch' in '{err}'"
    );
}
