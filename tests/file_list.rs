use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use mxd::{
    field_id::FieldId,
    transaction::{FrameHeader, Transaction, decode_params, encode_params},
    transaction_type::TransactionType,
};
use test_util::{handshake, setup_files_db};
mod common;

#[test]
fn list_files_acl() -> Result<(), Box<dyn std::error::Error>> {
    let Some(server) = common::start_server_or_skip(setup_files_db)? else {
        return Ok(());
    };

    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;

    handshake(&mut stream)?;

    // login
    let params = vec![
        (FieldId::Login, b"alice".as_ref()),
        (FieldId::Password, b"secret".as_ref()),
    ];
    let payload = encode_params(&params)?;
    let payload_len = u32::try_from(payload.len())
        .expect("payload length fits within the 32-bit header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::Login.into(),
        id: 1,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let reply_hdr = FrameHeader::from_bytes(&buf);
    let mut data = vec![0u8; reply_hdr.data_size as usize];
    stream.read_exact(&mut data)?;

    assert_eq!(reply_hdr.error, 0);

    // list files
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetFileNameList.into(),
        id: 2,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    let tx = Transaction {
        header,
        payload: Vec::new(),
    };
    stream.write_all(&tx.to_bytes())?;
    stream.read_exact(&mut buf)?;
    let hdr = FrameHeader::from_bytes(&buf);
    let mut payload = vec![0u8; hdr.data_size as usize];
    stream.read_exact(&mut payload)?;
    let resp = Transaction {
        header: hdr,
        payload,
    };
    assert_eq!(resp.header.error, 0);
    let params = decode_params(&resp.payload)?;
    let names: Vec<String> = params
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::FileName {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, vec!["fileA.txt", "fileC.txt"]);
    Ok(())
}

#[test]
fn list_files_reject_payload() -> Result<(), Box<dyn std::error::Error>> {
    let Some(server) = common::start_server_or_skip(|_| Ok(()))? else {
        return Ok(());
    };
    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;

    // handshake
    handshake(&mut stream)?;

    // send GetFileNameList with bogus payload
    let params = encode_params(&[(FieldId::Other(999), b"bogus".as_ref())])?;
    let params_len = u32::try_from(params.len())
        .expect("parameter block length fits within the header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetFileNameList.into(),
        id: 99,
        error: 0,
        total_size: params_len,
        data_size: params_len,
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
