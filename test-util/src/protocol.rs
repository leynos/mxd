//! Protocol helpers shared by integration tests.
//!
//! Currently provides the client-side handshake and login used by multiple suites.

use std::{
    io::{self, Read, Write},
    net::TcpStream,
};

use mxd::{
    field_id::FieldId,
    protocol::{HANDSHAKE_LEN, PROTOCOL_ID, REPLY_LEN, VERSION},
    transaction::{FrameHeader, Transaction, encode_params},
    transaction_type::TransactionType,
};

#[expect(
    clippy::big_endian_bytes,
    reason = "TRTP protocol uses network byte order"
)]
fn handshake_request(sub_version: u16) -> [u8; HANDSHAKE_LEN] {
    let mut request = [0u8; HANDSHAKE_LEN];
    request[0..4].copy_from_slice(PROTOCOL_ID.as_slice());
    request[4..8].copy_from_slice(&0u32.to_be_bytes());
    request[8..10].copy_from_slice(&VERSION.to_be_bytes());
    request[10..12].copy_from_slice(&sub_version.to_be_bytes());
    request
}

#[expect(
    clippy::big_endian_bytes,
    reason = "TRTP reply code uses network byte order"
)]
fn validate_handshake_reply(reply: [u8; REPLY_LEN]) -> io::Result<()> {
    if &reply[0..4] != PROTOCOL_ID.as_slice() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protocol mismatch in handshake reply",
        ));
    }
    let code_bytes: [u8; 4] = reply[4..8]
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "handshake reply too short"))?;
    let code = u32::from_be_bytes(code_bytes);
    if code != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("handshake returned error code {code}"),
        ));
    }
    Ok(())
}

/// Send a client handshake and verify the server reply matches expectations.
///
/// # Errors
///
/// Returns an I/O error if the handshake fails.
pub fn handshake(stream: &mut TcpStream) -> std::io::Result<()> {
    handshake_with_sub_version(stream, 0)
}

/// Send a client handshake with an explicit sub-version and validate reply.
///
/// # Errors
///
/// Returns an I/O error if the handshake write/read fails or if the server
/// reports a non-zero error code.
pub fn handshake_with_sub_version(stream: &mut TcpStream, sub_version: u16) -> std::io::Result<()> {
    let request = handshake_request(sub_version);
    stream.write_all(&request)?;
    let mut reply = [0u8; REPLY_LEN];
    stream.read_exact(&mut reply)?;
    validate_handshake_reply(reply)
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(0, [0u8, 0u8])]
    #[case(2, [0u8, 2u8])]
    fn handshake_request_encodes_sub_version(#[case] sub_version: u16, #[case] expected: [u8; 2]) {
        let request = handshake_request(sub_version);
        assert_eq!(&request[0..4], PROTOCOL_ID.as_slice());
        assert_eq!(&request[4..8], [0u8, 0u8, 0u8, 0u8].as_slice());
        let version_hi = (VERSION >> 8) as u8;
        let version_lo = (VERSION & 0x00ff) as u8;
        assert_eq!(&request[8..10], [version_hi, version_lo].as_slice());
        assert_eq!(&request[10..12], expected.as_slice());
    }

    #[rstest]
    fn validate_handshake_reply_accepts_success() {
        let mut reply = [0u8; REPLY_LEN];
        reply[0..4].copy_from_slice(PROTOCOL_ID.as_slice());
        assert!(validate_handshake_reply(reply).is_ok());
    }

    #[rstest]
    fn validate_handshake_reply_rejects_protocol_mismatch() {
        let mut reply = [0u8; REPLY_LEN];
        reply[0..4].copy_from_slice(b"NOPE");
        let err = validate_handshake_reply(reply).expect_err("expected protocol mismatch");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(err.to_string(), "protocol mismatch in handshake reply");
    }

    #[rstest]
    fn validate_handshake_reply_rejects_non_zero_error_code() {
        let mut reply = [0u8; REPLY_LEN];
        reply[0..4].copy_from_slice(PROTOCOL_ID.as_slice());
        reply[4..8].copy_from_slice([0u8, 0u8, 0u8, 3u8].as_slice());
        let err = validate_handshake_reply(reply).expect_err("expected non-zero handshake code");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(err.to_string(), "handshake returned error code 3");
    }
}
