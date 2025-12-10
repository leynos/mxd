#![allow(missing_docs, reason = "test file")]
#![allow(clippy::expect_used, reason = "test assertions")]
#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(clippy::panic_in_result_fn, reason = "test assertions")]
#![allow(clippy::big_endian_bytes, reason = "network protocol")]
#![allow(clippy::let_underscore_must_use, reason = "test cleanup")]

//! Exercises the legacy TCP handshake with an invalid protocol preamble.
//! Skipped when the `legacy-networking` feature is not compiled.
#![cfg(feature = "legacy-networking")]

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use test_util::AnyError;

mod common;

#[test]
fn handshake_invalid_protocol() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_| Ok(()))? else {
        return Ok(());
    };
    let port = server.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
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
