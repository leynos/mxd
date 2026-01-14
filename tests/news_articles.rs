//! Integration tests covering news article flows.
//!
//! Exercises listing, posting, and fetching news articles through the
//! wireframe transport.

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use diesel::prelude::*;
use diesel_async::{AsyncConnection, RunQueryDsl};
use mxd::{
    commands::NEWS_ERR_PATH_UNSUPPORTED,
    db::{DbConnection, create_category},
    field_id::FieldId,
    models::NewCategory,
    transaction::{FrameHeader, Transaction, decode_params, encode_params},
    transaction_type::TransactionType,
};
use test_util::{
    AnyError,
    ensure_test_user,
    handshake,
    login,
    setup_news_db,
    setup_news_with_article,
    with_db,
};
mod common;

type ParamList = Vec<(FieldId, Vec<u8>)>;

fn connect_and_handshake(port: u16) -> Result<TcpStream, AnyError> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    handshake(&mut stream)?;
    Ok(stream)
}

fn connect_handshake_and_login(port: u16) -> Result<TcpStream, AnyError> {
    let mut stream = connect_and_handshake(port)?;
    login(&mut stream, "alice", "secret")?;
    Ok(stream)
}

fn send_transaction(stream: &mut TcpStream, tx: &Transaction) -> Result<(), AnyError> {
    stream.write_all(&tx.to_bytes())?;
    Ok(())
}

#[expect(clippy::expect_used, reason = "infallible: payload size fits u32")]
fn send_transaction_with_params(
    stream: &mut TcpStream,
    ty: TransactionType,
    id: u32,
    params: &[(FieldId, &[u8])],
) -> Result<(), AnyError> {
    let payload = encode_params(params)?;
    let payload_size = u32::try_from(payload.len()).expect("payload fits in u32");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: ty.into(),
        id,
        error: 0,
        total_size: payload_size,
        data_size: payload_size,
    };
    let tx = Transaction { header, payload };
    send_transaction(stream, &tx)
}

fn read_response_header(stream: &mut TcpStream) -> Result<FrameHeader, AnyError> {
    let mut hdr_buf = [0u8; 20];
    stream.read_exact(&mut hdr_buf)?;
    Ok(FrameHeader::from_bytes(&hdr_buf))
}

fn read_response_payload(stream: &mut TcpStream, size: u32) -> Result<Vec<u8>, AnyError> {
    let mut data = vec![0u8; size as usize];
    stream.read_exact(&mut data)?;
    Ok(data)
}

fn receive_transaction(stream: &mut TcpStream) -> Result<(FrameHeader, ParamList), AnyError> {
    let hdr = read_response_header(stream)?;
    let payload = read_response_payload(stream, hdr.data_size)?;
    let params = decode_params(&payload)?;
    Ok((hdr, params))
}

#[expect(clippy::expect_used, reason = "test assertion helper")]
fn assert_field_utf8(params: &ParamList, field_id: FieldId, expected: &str, context: &str) {
    let bytes = params
        .iter()
        .find_map(|(id, data)| (id == &field_id).then_some(data))
        .expect("expected field not found in response");
    let value = String::from_utf8(bytes.clone()).expect("response field must contain valid UTF-8");
    assert_eq!(value, expected, "{context}");
}

#[expect(clippy::panic_in_result_fn, reason = "test assertion helper")]
fn verify_article_titles(db_url: &str, expected: &[&str]) -> Result<(), AnyError> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        use mxd::schema::news_articles::dsl as a;

        let mut conn = DbConnection::establish(db_url).await?;
        let titles = a::news_articles
            .select(a::title)
            .load::<String>(&mut conn)
            .await?;
        assert_eq!(titles, expected);
        Ok::<(), AnyError>(())
    })?;
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[test]
fn list_news_articles_invalid_path() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|db| {
        with_db(db, |conn| {
            Box::pin(async move {
                // Create test user for authentication
                ensure_test_user(conn).await?;

                create_category(
                    conn,
                    &NewCategory {
                        name: "General",
                        bundle_id: None,
                    },
                )
                .await?;
                Ok(())
            })
        })
    })?
    else {
        return Ok(());
    };

    let port = server.port();
    let mut stream = connect_handshake_and_login(port)?;
    send_transaction_with_params(
        &mut stream,
        TransactionType::NewsArticleNameList,
        6,
        &[(FieldId::NewsPath, b"Missing")],
    )?;

    let (hdr, _) = receive_transaction(&mut stream)?;
    assert_eq!(hdr.error, NEWS_ERR_PATH_UNSUPPORTED);
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[test]
fn list_news_articles_valid_path() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(setup_news_db)? else {
        return Ok(());
    };

    let mut stream = connect_handshake_and_login(server.port())?;
    send_transaction_with_params(
        &mut stream,
        TransactionType::NewsArticleNameList,
        7,
        &[(FieldId::NewsPath, b"General")],
    )?;

    let (_hdr, params) = receive_transaction(&mut stream)?;
    let names: Vec<String> = params
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::NewsArticle {
                Some(String::from_utf8(d).expect("reply contains valid UTF-8 for article name"))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, vec!["First", "Second"]);
    Ok(())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[test]
fn get_news_article_data() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(setup_news_with_article)? else {
        return Ok(());
    };

    let mut stream = connect_handshake_and_login(server.port())?;

    let mut params = Vec::new();
    params.push((FieldId::NewsPath, b"General".as_ref()));
    let id_bytes = 1i32.to_be_bytes();
    params.push((FieldId::NewsArticleId, id_bytes.as_ref()));
    params.push((FieldId::NewsDataFlavor, b"text/plain".as_ref()));
    send_transaction_with_params(&mut stream, TransactionType::NewsArticleData, 8, &params)?;

    let (_hdr, reply_params) = receive_transaction(&mut stream)?;
    assert_field_utf8(
        &reply_params,
        FieldId::NewsTitle,
        "First",
        "article title should round-trip",
    );
    assert_field_utf8(
        &reply_params,
        FieldId::NewsArticleData,
        "hello",
        "article data should round-trip",
    );
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[test]
fn post_news_article_root() -> Result<(), AnyError> {
    let Some(server) = common::start_server_or_skip(|db| {
        with_db(db, |conn| {
            Box::pin(async move {
                // Create test user for authentication
                ensure_test_user(conn).await?;

                create_category(
                    conn,
                    &NewCategory {
                        name: "General",
                        bundle_id: None,
                    },
                )
                .await?;
                Ok(())
            })
        })
    })?
    else {
        return Ok(());
    };

    let mut stream = connect_handshake_and_login(server.port())?;

    let mut request_params = Vec::new();
    request_params.push((FieldId::NewsPath, b"General".as_ref()));
    request_params.push((FieldId::NewsTitle, b"Hello".as_ref()));
    let flag_bytes = 0i32.to_be_bytes();
    request_params.push((FieldId::NewsArticleFlags, flag_bytes.as_ref()));
    request_params.push((FieldId::NewsDataFlavor, b"text/plain".as_ref()));
    request_params.push((FieldId::NewsArticleData, b"hi".as_ref()));
    let request_payload = encode_params(&request_params)?;
    let payload_size = u32::try_from(request_payload.len()).expect("payload fits in u32");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::PostNewsArticle.into(),
        id: 9,
        error: 0,
        total_size: payload_size,
        data_size: payload_size,
    };
    let tx = Transaction {
        header,
        payload: request_payload,
    };
    send_transaction(&mut stream, &tx)?;

    let hdr = read_response_header(&mut stream)?;
    assert_eq!(hdr.error, 0);
    let reply_payload = read_response_payload(&mut stream, hdr.data_size)?;
    let reply_params = decode_params(&reply_payload)?;
    let mut id_found = false;
    for (id, data) in reply_params {
        if id == FieldId::NewsArticleId {
            let arr: [u8; 4] = data
                .try_into()
                .expect("news-article id field contains exactly 4 bytes");
            assert_eq!(i32::from_be_bytes(arr), 1);
            id_found = true;
        }
    }
    assert!(id_found);

    verify_article_titles(server.db_url().as_str(), &["Hello"])?;
    Ok(())
}
