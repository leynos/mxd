use std::time::Duration;

use mxd::{field_id, transaction::*, transaction_type};
use rstest::rstest;
use tokio::io::{AsyncWriteExt, duplex};

fn build_tx() -> Transaction {
    let mut payload = Vec::new();
    payload.extend_from_slice(&[0x00, 0x02]);
    payload.extend_from_slice(&[0x00, 0x01, 0x00, 0x01, 0xff]);
    payload.extend_from_slice(&[0x00, 0x02, 0x00, 0x02, 0xaa, 0xbb]);
    let payload_len = u32::try_from(payload.len())
        .expect("payload length fits within the 32-bit header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 1,
        id: 1,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    Transaction { header, payload }
}

#[rstest]
#[tokio::test]
#[timeout(Duration::from_secs(20))]
async fn roundtrip_single_frame() {
    let tx = build_tx();
    let (mut a, mut b) = duplex(1024);
    let mut writer = TransactionWriter::new(&mut a);
    let mut reader = TransactionReader::new(&mut b);
    writer.write_transaction(&tx).await.unwrap();
    let rx = reader.read_transaction().await.unwrap();
    assert_eq!(tx, rx);
}

#[rstest]
#[tokio::test]
#[timeout(Duration::from_secs(20))]
async fn roundtrip_multi_frame() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&[0x00, 0x01]); // one param
    payload.extend_from_slice(&[0x00, 0x10]); // field id 16
    let big_size = u16::try_from(MAX_FRAME_DATA + 1)
        .expect("frame data limit fits within 16 bits when split");
    payload.extend_from_slice(&big_size.to_be_bytes());
    payload.extend(vec![0u8; big_size as usize]);
    let payload_len = u32::try_from(payload.len())
        .expect("payload length fits within the 32-bit header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 1,
        id: 2,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    let tx = Transaction { header, payload };
    let (mut a, mut b) = duplex(65536);
    let mut writer = TransactionWriter::new(&mut a);
    let mut reader = TransactionReader::new(&mut b);
    writer.write_transaction(&tx).await.unwrap();
    let rx = reader.read_transaction().await.unwrap();
    assert_eq!(tx, rx);
}

#[rstest]
#[tokio::test]
#[timeout(Duration::from_secs(20))]
async fn invalid_flags_error() {
    let mut tx = build_tx();
    tx.header.flags = 1;
    let (mut a, mut b) = duplex(1024);
    let mut writer = TransactionWriter::new(&mut a);
    let err = writer.write_transaction(&tx).await.unwrap_err();
    assert!(matches!(err, TransactionError::InvalidFlags));
    let mut buf = [0u8; HEADER_LEN];
    tx.header.write_bytes(&mut buf);
    a.write_all(&buf).await.unwrap();
    a.write_all(&tx.payload).await.unwrap();
    let mut reader = TransactionReader::new(&mut b);
    match reader.read_transaction().await.unwrap_err() {
        TransactionError::InvalidFlags => {}
        e => panic!("unexpected {e:?}"),
    }
}

#[rstest]
#[tokio::test]
#[timeout(Duration::from_secs(20))]
async fn mismatched_sizes() {
    let tx = build_tx();
    let (mut a, mut b) = duplex(1024);
    let mut header = tx.header.clone();
    header.total_size += 1;
    let mut buf = [0u8; HEADER_LEN];
    header.write_bytes(&mut buf);
    a.write_all(&buf).await.unwrap();
    a.write_all(&tx.payload).await.unwrap();
    // second fragment with invalid data_size
    header.data_size = 2;
    header.write_bytes(&mut buf);
    a.write_all(&buf).await.unwrap();
    a.write_all(&[0u8; 2]).await.unwrap();
    let mut reader = TransactionReader::new(&mut b);
    match reader.read_transaction().await.unwrap_err() {
        TransactionError::SizeMismatch => {}
        e => panic!("unexpected {e:?}"),
    }
}

#[rstest]
#[tokio::test]
#[timeout(Duration::from_secs(20))]
async fn duplicate_field_error() {
    let mut tx = build_tx();
    tx.payload
        .extend_from_slice(&[0x00, 0x01, 0x00, 0x01, 0xee]);
    tx.payload[0] = 0x00;
    tx.payload[1] = 0x03;
    tx.header.total_size = u32::try_from(tx.payload.len())
        .expect("payload length fits within the 32-bit header field");
    tx.header.data_size = tx.header.total_size;
    let (mut a, mut b) = duplex(MAX_PAYLOAD_SIZE * 2);
    let mut writer = TransactionWriter::new(&mut a);
    assert!(matches!(
        writer.write_transaction(&tx).await,
        Err(TransactionError::DuplicateField(1))
    ));
    let mut buf = [0u8; HEADER_LEN];
    tx.header.write_bytes(&mut buf);
    a.write_all(&buf).await.unwrap();
    a.write_all(&tx.payload).await.unwrap();
    let mut reader = TransactionReader::new(&mut b);
    match reader.read_transaction().await.unwrap_err() {
        TransactionError::DuplicateField(1) => {}
        e => panic!("unexpected {e:?}"),
    }
}

#[rstest]
#[tokio::test]
#[timeout(Duration::from_secs(20))]
async fn writer_payload_too_large() {
    let count = 16u16;
    let mut payload = Vec::new();
    payload.extend_from_slice(&count.to_be_bytes());
    for i in 0..count {
        payload.extend_from_slice(&(i + 1).to_be_bytes());
        payload.extend_from_slice(&0xffffu16.to_be_bytes());
        payload.extend(vec![0u8; 0xffff]);
    }
    let payload_len = u32::try_from(payload.len())
        .expect("payload length fits within the 32-bit header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 1,
        id: 99,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    let tx = Transaction { header, payload };
    let (mut w, _) = duplex(MAX_PAYLOAD_SIZE * 2);
    let mut writer = TransactionWriter::new(&mut w);
    match writer.write_transaction(&tx).await.unwrap_err() {
        TransactionError::PayloadTooLarge => {}
        e => panic!("unexpected {e:?}"),
    }
}

#[rstest]
#[tokio::test]
#[timeout(Duration::from_secs(20))]
async fn roundtrip_empty_payload() {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 3,
        id: 42,
        error: 0,
        total_size: 0,
        data_size: 0,
    };

    let tx = Transaction {
        header,
        payload: Vec::new(),
    };
    let (mut a, mut b) = duplex(1024);
    let mut writer = TransactionWriter::new(&mut a);
    let mut reader = TransactionReader::new(&mut b);
    writer.write_transaction(&tx).await.unwrap();
    let rx = reader.read_transaction().await.unwrap();
    assert_eq!(tx, rx);
}

#[rstest]
#[tokio::test]
#[timeout(Duration::from_secs(20))]
async fn oversized_payload() {
    let mut tx = build_tx();
    tx.payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];
    tx.header.total_size = u32::try_from(tx.payload.len())
        .expect("payload length fits within the 32-bit header field");
    tx.header.data_size = tx.header.total_size;
    let (mut a, mut b) = duplex(MAX_PAYLOAD_SIZE * 2);
    let mut buf = [0u8; HEADER_LEN];
    tx.header.write_bytes(&mut buf);
    a.write_all(&buf).await.unwrap();
    a.write_all(&tx.payload).await.unwrap();
    let mut reader = TransactionReader::new(&mut b);
    match reader.read_transaction().await.unwrap_err() {
        TransactionError::PayloadTooLarge => {}
        e => panic!("unexpected {e:?}"),
    }
}

#[test]
fn short_header_error() {
    let err = FrameHeader::new(&[0u8; 10]).unwrap_err();
    assert!(matches!(err, TransactionError::ShortBuffer));
}

#[test]
fn short_frame_error() {
    let buf = vec![0u8; HEADER_LEN - 2];
    match parse_transaction(&buf).unwrap_err() {
        TransactionError::SizeMismatch => {}
        e => panic!("unexpected {e:?}"),
    }
}

#[test]
fn parse_transaction_rejects_large_frame() {
    let payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];
    let payload_len = u32::try_from(payload.len())
        .expect("payload length fits within the 32-bit header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 9,
        id: 5,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    let tx = Transaction { header, payload };
    let frame = tx.to_bytes();
    match parse_transaction(&frame).unwrap_err() {
        TransactionError::PayloadTooLarge => {}
        e => panic!("unexpected {e:?}"),
    }
}

#[test]
fn display_field_and_type() {
    use field_id::FieldId;
    use transaction_type::TransactionType;
    assert_eq!(FieldId::Login.to_string(), "Login");
    assert_eq!(FieldId::Other(42).to_string(), "Other(42)");
    assert_eq!(TransactionType::Login.to_string(), "Login");
    assert_eq!(TransactionType::Other(99).to_string(), "Other(99)");
}

#[test]
fn duplicate_news_category_fields_allowed() {
    use field_id::FieldId;
    use transaction_type::TransactionType;
    let params = [
        (FieldId::NewsCategory, b"General".as_ref()),
        (FieldId::NewsCategory, b"Updates".as_ref()),
    ];
    let payload = encode_params(&params).unwrap();
    let payload_len = u32::try_from(payload.len())
        .expect("payload length fits within the 32-bit header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 5,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    let tx = Transaction { header, payload };
    let frame = tx.to_bytes();
    let parsed = parse_transaction(&frame).expect("parse");
    let decoded = decode_params(&parsed.payload).expect("decode");
    let names: Vec<String> = decoded
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::NewsCategory {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, ["General", "Updates"]);
}
