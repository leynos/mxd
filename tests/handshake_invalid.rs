//! Exercises the Hotline handshake with an invalid protocol preamble.

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use test_util::{AnyError, DatabaseUrl};

mod common;

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[test]
fn handshake_invalid_protocol() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_: DatabaseUrl| Ok(()))? else {
        return Ok(());
    };
    let addr = server.bind_addr();

    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    let mut handshake = Vec::new();
    handshake.extend_from_slice(b"WRNG");
    handshake.extend_from_slice(&0u32.to_be_bytes());
    handshake.extend_from_slice(&1u16.to_be_bytes());
    handshake.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&handshake)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(&reply[0..4], b"TRTP");
    assert_eq!(
        u32::from_be_bytes(
            reply[4..8]
                .try_into()
                .expect("reply slice must convert to 4-byte array")
        ),
        1
    );
    Ok(())
}
