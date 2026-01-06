#![expect(clippy::panic_in_result_fn, reason = "test assertions")]
#![expect(clippy::big_endian_bytes, reason = "network protocol")]

//! Handshake integration tests covering unsupported protocol versions.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    time::Duration,
};

use test_util::AnyError;

mod common;

#[test]
fn handshake_unsupported_version() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_| Ok(()))? else {
        return Ok(());
    };
    let port = server.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    let mut handshake = Vec::new();
    handshake.extend_from_slice(b"TRTP");
    handshake.extend_from_slice(&0u32.to_be_bytes());
    handshake.extend_from_slice(&2u16.to_be_bytes());
    handshake.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&handshake)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(&reply[0..4], b"TRTP");
    assert_eq!(
        u32::from_be_bytes(
            reply[4..8]
                .try_into()
                .expect("slice is 4 bytes due to reply buffer length"),
        ),
        2,
    );

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    assert_eq!(bytes, 0, "unexpected server data: {line}");
    Ok(())
}
