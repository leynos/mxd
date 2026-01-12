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

/// Assert that encoding the given transaction fails with an error message
/// containing the expected substring.
fn assert_encode_error(tx: &HotlineTransaction, expected_msg: &str) {
    let err = encode_to_vec(tx, hotline_config()).expect_err("encode must fail");
    assert!(
        err.to_string().contains(expected_msg),
        "expected '{expected_msg}' in '{err}'"
    );
}

/// Assert that decoding a transaction with the given header and payload fails
/// with an error message containing the expected substring.
async fn assert_decode_error(header: FrameHeader, payload: Vec<u8>, expected_msg: &str) {
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
    assert_decode_error(header, payload, expected_msg).await;
}

#[rstest]
#[case::invalid_flags(
    FrameHeader {
        flags: 1,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 0,
        data_size: 0,
    },
    Vec::new(),
    "invalid flags"
)]
#[case::oversized_total(
    FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: u32::try_from(MAX_PAYLOAD_SIZE + 1).expect("test size fits in u32"),
        data_size: u32::try_from(MAX_FRAME_DATA).expect("frame data fits in u32"),
    },
    vec![0u8; MAX_FRAME_DATA],
    "total size exceeds maximum"
)]
#[case::oversized_data(
    FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: u32::try_from(MAX_FRAME_DATA + 1).expect("test size fits in u32"),
        data_size: u32::try_from(MAX_FRAME_DATA + 1).expect("test size fits in u32"),
    },
    vec![0u8; MAX_FRAME_DATA + 1],
    "data size exceeds maximum"
)]
#[tokio::test]
async fn rejects_invalid_headers(
    #[case] header: FrameHeader,
    #[case] payload: Vec<u8>,
    #[case] expected_msg: &str,
) {
    assert_decode_error(header, payload, expected_msg).await;
}

#[rstest]
#[case(0)]
#[case(MAX_FRAME_DATA)]
#[case(MAX_FRAME_DATA + 1)]
#[case(2 * MAX_FRAME_DATA + 1)]
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

    let expected = expected_framing_bytes(&header, &payload);

    assert_eq!(bytes, expected);
}

/// Construct the expected wire framing for the given header and payload.
///
/// Mirrors the codec's framing behaviour by emitting a single header-only frame
/// for an empty payload, or fragmenting non-empty payloads into chunks of
/// `MAX_FRAME_DATA` and stamping each frame with the adjusted `data_size`.
fn expected_framing_bytes(header: &FrameHeader, payload: &[u8]) -> Vec<u8> {
    let mut expected = Vec::new();
    if payload.is_empty() {
        expected.extend(transaction_bytes(
            &FrameHeader {
                data_size: 0,
                ..header.clone()
            },
            &[],
        ));
        return expected;
    }

    let mut offset = 0usize;
    while offset < payload.len() {
        let end = (offset + MAX_FRAME_DATA).min(payload.len());
        let chunk = payload
            .get(offset..end)
            .expect("payload length checked before slicing");
        let mut frame_header = header.clone();
        frame_header.data_size = u32::try_from(chunk.len()).expect("chunk length fits u32");
        expected.extend(transaction_bytes(&frame_header, chunk));
        offset = end;
    }

    expected
}

#[rstest]
#[case(0, 0)]
#[case(1, 1)]
#[case(MAX_FRAME_DATA, 1)]
#[case(MAX_FRAME_DATA + 1, 2)]
#[case((2 * MAX_FRAME_DATA) + 1, 3)]
fn fragment_ranges_cover_payload(#[case] total_len: usize, #[case] expected_count: usize) {
    let mut sum = 0usize;
    let mut count = 0usize;
    let mut last_len = 0usize;

    for (offset, len) in fragment_ranges(total_len) {
        assert!(len > 0);
        assert!(len <= MAX_FRAME_DATA);
        assert!(offset + len <= total_len);
        sum += len;
        count += 1;
        last_len = len;
    }

    assert_eq!(sum, total_len);
    assert_eq!(count, expected_count);
    if total_len == 0 {
        assert_eq!(last_len, 0);
    } else {
        assert!(last_len > 0);
    }
}

#[rstest]
#[case::size_mismatch(
    FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 2,
        data_size: 2,
    },
    Vec::new(),
    "size mismatch"
)]
#[case::invalid_flags(
    FrameHeader {
        flags: 1,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 0,
        data_size: 0,
    },
    Vec::new(),
    "invalid flags"
)]
#[case::oversized_payload(
    FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: u32::try_from(MAX_PAYLOAD_SIZE + 1).expect("test size fits in u32"),
        data_size: u32::try_from(MAX_PAYLOAD_SIZE + 1).expect("test size fits in u32"),
    },
    vec![0u8; MAX_PAYLOAD_SIZE + 1],
    "payload too large"
)]
fn encoding_rejects_invalid_transactions(
    #[case] header: FrameHeader,
    #[case] payload: Vec<u8>,
    #[case] expected_msg: &str,
) {
    let tx = HotlineTransaction { header, payload };
    assert_encode_error(&tx, expected_msg);
}
