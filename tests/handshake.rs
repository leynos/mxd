use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};

use test_util::TestServer;

#[test]
fn handshake() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start("./Cargo.toml")?;
    let port = server.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let mut handshake = Vec::new();
    handshake.extend_from_slice(b"TRTP");
    handshake.extend_from_slice(&0u32.to_be_bytes());
    handshake.extend_from_slice(&1u16.to_be_bytes());
    handshake.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&handshake)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(&reply[0..4], b"TRTP");
    assert_eq!(u32::from_be_bytes(reply[4..8].try_into().unwrap()), 0);

    // Close the write side to signal that no further data will be sent.
    // This allows the server to terminate the connection immediately
    // instead of waiting for a read timeout.
    stream.shutdown(Shutdown::Write)?;

    let mut tmp = [0u8; 1];
    assert_eq!(stream.read(&mut tmp)?, 0);
    Ok(())
}
