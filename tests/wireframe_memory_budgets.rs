#![expect(clippy::panic_in_result_fn, reason = "test assertions")]

//! Integration tests for Wireframe memory-budget enforcement.

use std::{
    io::{self, Read, Write},
    net::TcpStream,
    thread::sleep,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use mxd::{
    commands::ERR_INTERNAL_SERVER,
    field_id::FieldId,
    transaction::{
        FrameHeader,
        HEADER_LEN,
        IO_TIMEOUT,
        MAX_FRAME_DATA,
        MAX_PAYLOAD_SIZE,
        encode_params,
    },
    transaction_type::TransactionType,
    wireframe::test_helpers::{fragmented_transaction_bytes, transaction_bytes},
};
use test_util::{AnyError, DatabaseUrl, handshake};

mod common;

const UNKNOWN_TRANSACTION_TYPE: TransactionType = TransactionType::Other(900);
const LARGE_REQUEST_ID: u32 = 7001;
const SOFT_PRESSURE_PAYLOAD_BYTES: usize = MAX_PAYLOAD_SIZE - (MAX_FRAME_DATA * 2);
const HALF_FRAME_DATA: usize = MAX_FRAME_DATA >> 1;

fn connect_and_handshake(addr: std::net::SocketAddr) -> Result<TcpStream, AnyError> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    handshake(&mut stream)?;
    Ok(stream)
}

fn build_payload_at_least(minimum_payload_len: usize) -> Result<Vec<u8>, AnyError> {
    if minimum_payload_len > MAX_PAYLOAD_SIZE {
        return Err(anyhow!(
            "minimum payload {minimum_payload_len} exceeds MAX_PAYLOAD_SIZE"
        ));
    }

    let mut params = Vec::new();
    let mut encoded_len = 2usize;
    while encoded_len < minimum_payload_len {
        let remaining = minimum_payload_len - encoded_len;
        if remaining <= 4 {
            break;
        }
        let field_len = remaining.saturating_sub(4).min(usize::from(u16::MAX));
        params.push((FieldId::FileName, vec![b'a'; field_len]));
        encoded_len += 4 + field_len;
    }

    let payload = encode_params(&params)?;
    if payload.len() < minimum_payload_len {
        return Err(anyhow!(
            "payload builder undershot: expected at least {minimum_payload_len}, got {}",
            payload.len()
        ));
    }
    Ok(payload)
}

fn fragmented_request(
    payload: &[u8],
    id: u32,
    fragment_size: usize,
) -> Result<Vec<Vec<u8>>, AnyError> {
    let payload_len = u32::try_from(payload.len())?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: UNKNOWN_TRANSACTION_TYPE.into(),
        id,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    fragmented_transaction_bytes(&header, payload, fragment_size).map_err(Into::into)
}

fn read_reply_header(stream: &mut TcpStream) -> Result<FrameHeader, AnyError> {
    let mut header_bytes = [0u8; HEADER_LEN];
    stream.read_exact(&mut header_bytes)?;
    Ok(FrameHeader::from_bytes(&header_bytes))
}

fn is_retryable_io(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

fn is_closed_io(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::UnexpectedEof
    )
}

fn assert_connection_closed(stream: &mut TcpStream) -> Result<(), AnyError> {
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut probe = [0u8; 1];

    loop {
        match stream.read(&mut probe) {
            Ok(0) => return Ok(()),
            Ok(bytes_read) => {
                return Err(anyhow!(
                    "expected closed connection, read {bytes_read} unexpected byte(s)"
                ));
            }
            Err(error) if is_closed_io(&error) => return Ok(()),
            Err(error) if is_retryable_io(&error) && Instant::now() < deadline => {
                sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[test]
fn fragmented_request_within_explicit_budget_still_routes() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_: DatabaseUrl| Ok(()))? else {
        return Ok(());
    };
    let mut stream = connect_and_handshake(server.bind_addr())?;
    let payload = build_payload_at_least(SOFT_PRESSURE_PAYLOAD_BYTES)?;
    let fragments = fragmented_request(&payload, LARGE_REQUEST_ID, HALF_FRAME_DATA)?;

    for fragment in fragments {
        stream.write_all(&fragment)?;
    }

    let reply = read_reply_header(&mut stream)?;
    assert_eq!(reply.is_reply, 1);
    assert_eq!(reply.ty, u16::from(UNKNOWN_TRANSACTION_TYPE));
    assert_eq!(reply.id, LARGE_REQUEST_ID);
    assert_eq!(reply.error, ERR_INTERNAL_SERVER);
    assert_eq!(reply.total_size, 0);
    assert_eq!(reply.data_size, 0);
    Ok(())
}

#[test]
fn oversized_fragmented_request_is_disconnected_on_first_frame() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_: DatabaseUrl| Ok(()))? else {
        return Ok(());
    };
    let mut stream = connect_and_handshake(server.bind_addr())?;
    let oversized_total = u32::try_from(MAX_PAYLOAD_SIZE + 1)?;
    let first_chunk_len = MAX_FRAME_DATA;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: UNKNOWN_TRANSACTION_TYPE.into(),
        id: 7002,
        error: 0,
        total_size: oversized_total,
        data_size: u32::try_from(first_chunk_len)?,
    };
    let first_frame = transaction_bytes(&header, &vec![0u8; first_chunk_len]);

    stream.write_all(&first_frame)?;

    assert_connection_closed(&mut stream)
}

#[test]
fn stalled_fragmented_request_is_disconnected_when_continuation_resumes() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_: DatabaseUrl| Ok(()))? else {
        return Ok(());
    };
    let mut stream = connect_and_handshake(server.bind_addr())?;
    let payload = build_payload_at_least(MAX_FRAME_DATA + 1024)?;
    let fragments = fragmented_request(&payload, 7003, MAX_FRAME_DATA)?;

    assert!(
        fragments.len() >= 2,
        "test fixture must produce multiple fragments"
    );

    let Some(first_fragment) = fragments.first() else {
        return Err(anyhow!("expected at least one fragment"));
    };
    let Some(second_fragment) = fragments.get(1) else {
        return Err(anyhow!("expected a continuation fragment"));
    };

    stream.write_all(first_fragment)?;
    sleep(IO_TIMEOUT + Duration::from_millis(250));

    match stream.write_all(second_fragment) {
        Ok(()) => assert_connection_closed(&mut stream),
        Err(error) if is_closed_io(&error) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
