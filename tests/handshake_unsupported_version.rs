use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

use test_util::TestServer;

#[test]
fn handshake_unsupported_version() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start("./Cargo.toml")?;
    let port = server.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let mut handshake = Vec::new();
    handshake.extend_from_slice(b"TRTP");
    handshake.extend_from_slice(&0u32.to_be_bytes());
    handshake.extend_from_slice(&2u16.to_be_bytes());
    handshake.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&handshake)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(&reply[0..4], b"TRTP");
    assert_eq!(u32::from_be_bytes(reply[4..8].try_into().unwrap()), 2);

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    assert_eq!(bytes, 0, "unexpected server data: {}", line);
    Ok(())
}
