use std::io::{Read, Write};
use std::net::TcpStream;

use diesel::prelude::*;
use diesel_async::{AsyncConnection, RunQueryDsl};
use mxd::commands::NEWS_ERR_PATH_UNSUPPORTED;
use mxd::db::{DbConnection, create_category, run_migrations};
use mxd::field_id::FieldId;
use mxd::models::NewCategory;
use mxd::transaction::{FrameHeader, Transaction, decode_params, encode_params};
use mxd::transaction_type::TransactionType;
use test_util::TestServer;

#[test]
fn list_news_articles_invalid_path() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await?;
            run_migrations(&mut conn).await?;
            create_category(
                &mut conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            Ok(())
        })
    })?;

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

    let params = vec![(FieldId::NewsPath, b"Missing".as_ref())];
    let payload = encode_params(&params);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsArticleNameList.into(),
        id: 6,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;

    let mut hdr_buf = [0u8; 20];
    stream.read_exact(&mut hdr_buf)?;
    let hdr = FrameHeader::from_bytes(&hdr_buf);
    assert_eq!(hdr.error, NEWS_ERR_PATH_UNSUPPORTED);
    Ok(())
}

#[test]
fn list_news_articles_valid_path() -> Result<(), Box<dyn std::error::Error>> {
    use chrono::NaiveDateTime;
    use mxd::models::NewArticle;

    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await?;
            run_migrations(&mut conn).await?;
            create_category(
                &mut conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            use mxd::schema::news_articles::dsl as a;
            let posted = NaiveDateTime::from_timestamp_opt(1000, 0).unwrap();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: None,
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "First",
                    poster: None,
                    posted_at: posted,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("a"),
                })
                .execute(&mut conn)
                .await?;
            let posted2 = NaiveDateTime::from_timestamp_opt(2000, 0).unwrap();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: Some(1),
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "Second",
                    poster: None,
                    posted_at: posted2,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("b"),
                })
                .execute(&mut conn)
                .await?;
            Ok(())
        })
    })?;

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

    let params = vec![(FieldId::NewsPath, b"General".as_ref())];
    let payload = encode_params(&params);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsArticleNameList.into(),
        id: 7,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;

    let mut hdr_buf = [0u8; 20];
    stream.read_exact(&mut hdr_buf)?;
    let hdr = FrameHeader::from_bytes(&hdr_buf);
    let mut data = vec![0u8; hdr.data_size as usize];
    stream.read_exact(&mut data)?;
    let reply_tx = Transaction {
        header: hdr,
        payload: data,
    };
    let params = decode_params(&reply_tx.payload)?;
    let names: Vec<String> = params
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::NewsArticle {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, vec!["First", "Second"]);
    Ok(())
}

#[test]
fn get_news_article_data() -> Result<(), Box<dyn std::error::Error>> {
    use chrono::NaiveDateTime;
    use mxd::models::NewArticle;

    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await?;
            run_migrations(&mut conn).await?;
            create_category(
                &mut conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            use mxd::schema::news_articles::dsl as a;
            let posted = NaiveDateTime::from_timestamp_opt(1000, 0).unwrap();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: None,
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "First",
                    poster: Some("alice"),
                    posted_at: posted,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("hello"),
                })
                .execute(&mut conn)
                .await?;
            Ok(())
        })
    })?;

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

    let mut params = Vec::new();
    params.push((FieldId::NewsPath, b"General".as_ref()));
    let id_bytes = 1i32.to_be_bytes();
    params.push((FieldId::NewsArticleId, id_bytes.as_ref()));
    params.push((FieldId::NewsDataFlavor, b"text/plain".as_ref()));
    let payload = encode_params(&params);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsArticleData.into(),
        id: 8,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;

    let mut hdr_buf = [0u8; 20];
    stream.read_exact(&mut hdr_buf)?;
    let hdr = FrameHeader::from_bytes(&hdr_buf);
    let mut data = vec![0u8; hdr.data_size as usize];
    stream.read_exact(&mut data)?;
    let reply_tx = Transaction {
        header: hdr,
        payload: data,
    };
    let params = decode_params(&reply_tx.payload)?;
    let mut found_title = false;
    let mut found_data = false;
    for (id, d) in params {
        match id {
            FieldId::NewsTitle => {
                assert_eq!(String::from_utf8(d).unwrap(), "First");
                found_title = true;
            }
            FieldId::NewsArticleData => {
                assert_eq!(String::from_utf8(d).unwrap(), "hello");
                found_data = true;
            }
            _ => {}
        }
    }
    assert!(found_title && found_data);
    Ok(())
}
