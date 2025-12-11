#![expect(clippy::panic_in_result_fn, reason = "test assertions")]
#![expect(clippy::big_endian_bytes, reason = "network protocol")]

//! Handshake integration tests for the legacy TCP adapter.
//! Skips when the build omits the `legacy-networking` runtime.
#![cfg(feature = "legacy-networking")]

use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};

use test_util::AnyError;

mod common;

#[test]
fn handshake() -> Result<(), AnyError> {
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
                .expect("failed to decode reply length bytes"),
        ),
        0
    );

    // Close the write side to signal that no further data will be sent.
    // This allows the server to terminate the connection immediately
    // instead of waiting for a read timeout.
    stream.shutdown(Shutdown::Write)?;

    let mut tmp = [0u8; 1];
    assert_eq!(stream.read(&mut tmp)?, 0);
    Ok(())
}
