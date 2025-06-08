use std::io::{Read, Write};
use std::net::TcpStream;

use mxd::field_id::FieldId;
use mxd::transaction::{FrameHeader, Transaction, encode_params};
use mxd::transaction_type::TransactionType;
use test_util::TestServer;

fn handshake(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"TRTP");
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&buf)?;
    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    Ok(())
}

#[test]
fn download_banner_reject_payload() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start("./Cargo.toml")?;
    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    handshake(&mut stream)?;

    let params = encode_params(&[(FieldId::Other(1), b"bogus".as_ref())]);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::DownloadBanner.into(),
        id: 10,
        error: 0,
        total_size: params.len() as u32,
        data_size: params.len() as u32,
    };
    let tx = Transaction {
        header,
        payload: params,
    };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let hdr = FrameHeader::from_bytes(&buf);
    assert_eq!(hdr.error, 1);
    assert_eq!(hdr.data_size, 0);
    Ok(())
}

#[test]
fn user_name_list_reject_payload() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start("./Cargo.toml")?;
    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    handshake(&mut stream)?;

    let params = encode_params(&[(FieldId::Other(1), b"bogus".as_ref())]);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetUserNameList.into(),
        id: 11,
        error: 0,
        total_size: params.len() as u32,
        data_size: params.len() as u32,
    };
    let tx = Transaction {
        header,
        payload: params,
    };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let hdr = FrameHeader::from_bytes(&buf);
    assert_eq!(hdr.error, 1);
    assert_eq!(hdr.data_size, 0);
    Ok(())
}
