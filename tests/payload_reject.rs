#![expect(clippy::expect_used, reason = "test assertions")]
#![expect(clippy::panic_in_result_fn, reason = "test assertions")]
#![expect(clippy::big_endian_bytes, reason = "network protocol")]

//! Integration tests that reject malformed payloads for banner and
//! user list transactions.

use std::{
    convert::TryFrom,
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use mxd::{
    field_id::FieldId,
    transaction::{FrameHeader, Transaction, encode_params},
    transaction_type::TransactionType,
};
use test_util::{AnyError, DatabaseUrl};
mod common;

fn handshake(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"TRTP");
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&buf)?;
    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(
        &reply[0..4],
        b"TRTP",
        "protocol mismatch in handshake reply"
    );
    let code = u32::from_be_bytes(
        reply[4..8]
            .try_into()
            .expect("handshake reply contains a 4-byte status field"),
    );
    assert_eq!(code, 0, "handshake returned error code {code}");
    Ok(())
}

#[test]
fn download_banner_reject_payload() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_: DatabaseUrl| Ok(()))? else {
        return Ok(());
    };
    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    handshake(&mut stream)?;

    let params = encode_params(&[(FieldId::Other(1), b"bogus".as_ref())])?;
    let size = u32::try_from(params.len()).expect("params fit in u32");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::DownloadBanner.into(),
        id: 10,
        error: 0,
        total_size: size,
        data_size: size,
    };
    let tx = Transaction {
        header,
        payload: params,
    };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let hdr = FrameHeader::from_bytes(&buf);
    assert_eq!(hdr.error, mxd::commands::ERR_INVALID_PAYLOAD);
    assert_eq!(hdr.data_size, 0);
    Ok(())
}

#[test]
fn user_name_list_reject_payload() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_: DatabaseUrl| Ok(()))? else {
        return Ok(());
    };
    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    handshake(&mut stream)?;

    let params = encode_params(&[(FieldId::Other(1), b"bogus".as_ref())])?;
    let size = u32::try_from(params.len()).expect("params fit in u32");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetUserNameList.into(),
        id: 11,
        error: 0,
        total_size: size,
        data_size: size,
    };
    let tx = Transaction {
        header,
        payload: params,
    };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let hdr = FrameHeader::from_bytes(&buf);
    assert_eq!(hdr.error, mxd::commands::ERR_INVALID_PAYLOAD);
    assert_eq!(hdr.data_size, 0);
    Ok(())
}
