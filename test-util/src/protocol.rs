//! Protocol helpers shared by integration tests.
//!
//! Currently provides the client-side handshake used by multiple suites.

use std::{
    convert::TryInto,
    io::{self, Read, Write},
    net::TcpStream,
};

/// Send a client handshake and verify the server reply matches expectations.
pub fn handshake(stream: &mut TcpStream) -> std::io::Result<()> {
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
    let code_bytes: [u8; 4] = reply[4..8]
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "handshake reply too short"))?;
    let code = u32::from_be_bytes(code_bytes);
    assert_eq!(code, 0, "handshake returned error code {}", code);
    Ok(())
}
