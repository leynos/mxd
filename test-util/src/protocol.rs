//! Protocol helpers shared by integration tests.
//!
//! Currently provides the client-side handshake and login used by multiple suites.

use std::{
    convert::TryInto,
    io::{self, Read, Write},
    net::TcpStream,
};

use mxd::{
    field_id::FieldId,
    transaction::{FrameHeader, Transaction, encode_params},
    transaction_type::TransactionType,
};

/// Send a client handshake and verify the server reply matches expectations.
///
/// # Errors
///
/// Returns an I/O error if the handshake fails.
///
/// # Panics
///
/// Panics if the protocol magic doesn't match or the server returns an error code.
#[expect(
    clippy::big_endian_bytes,
    reason = "TRTP protocol uses network byte order"
)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "test helper: panics indicate protocol violations"
)]
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
    assert_eq!(code, 0, "handshake returned error code {code}");
    Ok(())
}

/// Send a login transaction and verify successful authentication.
///
/// # Errors
///
/// Returns an I/O error if the login fails.
///
/// # Panics
///
/// Panics if the server returns a non-zero error code (authentication failed).
#[expect(
    clippy::panic_in_result_fn,
    reason = "test helper: panics indicate protocol violations"
)]
pub fn login(stream: &mut TcpStream, username: &str, password: &str) -> std::io::Result<()> {
    let params: &[(FieldId, &[u8])] = &[
        (FieldId::Login, username.as_bytes()),
        (FieldId::Password, password.as_bytes()),
    ];
    let payload = encode_params(params)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let payload_size = u32::try_from(payload.len())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::Login.into(),
        id: 1,
        error: 0,
        total_size: payload_size,
        data_size: payload_size,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;

    // Read the response header
    let mut hdr_buf = [0u8; 20];
    stream.read_exact(&mut hdr_buf)?;
    let reply_header = FrameHeader::from_bytes(&hdr_buf);

    // Read the response payload (if any)
    if reply_header.data_size > 0 {
        let mut payload_buf = vec![0u8; reply_header.data_size as usize];
        stream.read_exact(&mut payload_buf)?;
    }

    assert_eq!(
        reply_header.error, 0,
        "login failed with error code {}",
        reply_header.error
    );
    Ok(())
}
