//! Integration tests for file list operations.
//!
//! Exercises the `GetFileNameList` transaction with ACL filtering and error
//! handling scenarios.
#![allow(
    unfulfilled_lint_expectations,
    reason = "test lint expectations may not all trigger"
)]
#![expect(clippy::expect_used, reason = "test assertions")]
#![expect(clippy::panic_in_result_fn, reason = "test assertions")]

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use mxd::{
    field_id::FieldId,
    transaction::{FrameHeader, Transaction, decode_params, encode_params},
    transaction_type::TransactionType,
};
use rstest::{fixture, rstest};
use test_util::{AnyError, TestServer, handshake, setup_files_db};
use tracing::{debug, info};
mod common;

type TestResult<T> = Result<T, AnyError>;
type SetupFn = fn(&str) -> TestResult<()>;

/// Performs the login transaction for a test connection.
///
/// # Examples
/// ```no_run
/// # use std::net::TcpStream;
/// # use test_util::AnyError;
/// # fn demo() -> Result<(), AnyError> {
/// # let mut stream = TcpStream::connect(("127.0.0.1", 9999))?;
/// perform_login(&mut stream, b"alice", b"secret")?;
/// # Ok(())
/// # }
/// ```
fn perform_login(stream: &mut TcpStream, username: &[u8], password: &[u8]) -> Result<(), AnyError> {
    let params = vec![(FieldId::Login, username), (FieldId::Password, password)];
    let payload = encode_params(&params)?;
    let payload_len =
        u32::try_from(payload.len()).expect("payload length fits within the 32-bit header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::Login.into(),
        id: 1,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let reply_hdr = FrameHeader::from_bytes(&buf);
    let mut data = vec![0u8; reply_hdr.data_size as usize];
    stream.read_exact(&mut data)?;

    if reply_hdr.error != 0 {
        return Err(format!("login failed with error code {}", reply_hdr.error).into());
    }

    Ok(())
}

/// Requests the remote file list and returns the decoded filenames.
///
/// # Examples
/// ```no_run
/// # use std::net::TcpStream;
/// # use test_util::AnyError;
/// # fn demo() -> Result<(), AnyError> {
/// # let mut stream = TcpStream::connect(("127.0.0.1", 9999))?;
/// let names = get_file_list(&mut stream)?;
/// # assert!(names.is_empty());
/// # Ok(())
/// # }
/// ```
fn get_file_list(stream: &mut TcpStream) -> Result<Vec<String>, AnyError> {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetFileNameList.into(),
        id: 2,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    let tx = Transaction {
        header,
        payload: Vec::new(),
    };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let hdr = FrameHeader::from_bytes(&buf);
    let mut payload = vec![0u8; hdr.data_size as usize];
    stream.read_exact(&mut payload)?;
    let resp = Transaction {
        header: hdr,
        payload,
    };
    if resp.header.error != 0 {
        return Err(format!(
            "file list request failed with error code {}",
            resp.header.error
        )
        .into());
    }
    let params = decode_params(&resp.payload)?;
    let mut names = Vec::new();
    for (id, data) in params {
        if id == FieldId::FileName {
            let name = String::from_utf8(data).map_err(|e| -> AnyError { e.into() })?;
            names.push(name);
        }
    }

    Ok(names)
}

#[fixture]
fn test_stream(
    #[default(setup_files_db as SetupFn)] setup: SetupFn,
) -> TestResult<Option<(TestServer, TcpStream)>> {
    let Some(server) = common::start_server_or_skip(setup)? else {
        return Ok(None);
    };
    let port = server.port();
    debug!(port, "connecting to server");
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    debug!("connected, setting timeouts");
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;

    debug!("performing handshake");
    handshake(&mut stream)?;
    info!("handshake complete");
    Ok(Some((server, stream)))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Keep signature consistent with `SetupFn` so tests can swap in fallible setup \
              routines."
)]
fn noop_setup(_: &str) -> TestResult<()> { Ok(()) }

#[rstest]
fn list_files_acl(test_stream: TestResult<Option<(TestServer, TcpStream)>>) -> TestResult<()> {
    let Some((server, mut stream)) = test_stream? else {
        return Ok(());
    };
    let _server = server;
    debug!("performing login");
    perform_login(&mut stream, b"alice", b"secret")?;
    debug!("login complete, getting file list");
    let names = get_file_list(&mut stream)?;
    info!(files = ?names, "got file list");
    assert_eq!(names, vec!["fileA.txt", "fileC.txt"]);
    Ok(())
}

#[rstest]
fn list_files_reject_payload(
    #[with(noop_setup)] test_stream: TestResult<Option<(TestServer, TcpStream)>>,
) -> TestResult<()> {
    let Some((server, mut stream)) = test_stream? else {
        return Ok(());
    };
    let _server = server;
    // send GetFileNameList with bogus payload
    let params = encode_params(&[(FieldId::Other(999), b"bogus".as_ref())])?;
    let params_len =
        u32::try_from(params.len()).expect("parameter block length fits within the header field");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetFileNameList.into(),
        id: 99,
        error: 0,
        total_size: params_len,
        data_size: params_len,
    };
    let tx = Transaction {
        header,
        payload: params,
    };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let hdr = FrameHeader::from_bytes(&buf);
    assert_eq!(hdr.error, mxd::commands::ERR_INVALID_PAYLOAD);
    assert_eq!(hdr.data_size, 0);
    Ok(())
}
