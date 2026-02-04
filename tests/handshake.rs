//! Handshake integration tests for the Hotline server.
#![expect(clippy::big_endian_bytes, reason = "network protocol")]

use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};

use test_util::{AnyError, DatabaseUrl};

mod common;

#[test]
fn handshake() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|_: DatabaseUrl| Ok(()))? else {
        return Ok(());
    };
    let addr = server.bind_addr();

    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    let mut handshake = Vec::new();
    handshake.extend_from_slice(b"TRTP");
    handshake.extend_from_slice(&[0, 0, 0, 0]); // length = 0
    handshake.extend_from_slice(&[0, 1]); // version = 1
    handshake.extend_from_slice(&[0, 0]); // reserved/flags = 0
    stream.write_all(&handshake)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    if &reply[0..4] != b"TRTP" {
        return Err(anyhow::anyhow!("unexpected reply magic"));
    }

    let status_bytes: [u8; 4] = reply[4..8]
        .try_into()
        .map_err(|_| anyhow::anyhow!("failed to decode reply status bytes"))?;
    let status = u32::from_be_bytes(status_bytes);
    if status != 0 {
        return Err(anyhow::anyhow!("expected zero status, got {status}"));
    }

    // Close the write side to signal that no further data will be sent.
    // This allows the server to terminate the connection immediately
    // instead of waiting for a read timeout.
    stream.shutdown(Shutdown::Write)?;

    let mut tmp = [0u8; 1];
    if stream.read(&mut tmp)? != 0 {
        return Err(anyhow::anyhow!("expected EOF after handshake"));
    }
    Ok(())
}
