use std::io::{Read, Write};
use std::net::TcpStream;

use diesel::prelude::*;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;
use mxd::commands::NEWS_ERR_PATH_UNSUPPORTED;
use mxd::db::DbConnection;
use mxd::db::create_category;
use mxd::field_id::FieldId;
use mxd::models::NewCategory;
use mxd::transaction::{FrameHeader, Transaction, decode_params, encode_params};
use mxd::transaction_type::TransactionType;
use test_util::{TestServer, handshake, setup_news_db, with_db};

#[test]
fn list_news_articles_invalid_path() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        with_db(db, |conn| {
            Box::pin(async move {
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
    })?;

    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    handshake(&mut stream)?;

    let params = vec![(FieldId::NewsPath, b"Missing".as_ref())];
    let payload = encode_params(&params)?;
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
    let server = TestServer::start_with_setup("./Cargo.toml", |db| setup_news_db(db))?;

    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    handshake(&mut stream)?;

    let params = vec![(FieldId::NewsPath, b"General".as_ref())];
    let payload = encode_params(&params)?;
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
    use chrono::{DateTime, Utc};
    use mxd::models::NewArticle;

    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        with_db(db, |conn| {
            Box::pin(async move {
                create_category(
                    conn,
                    &NewCategory {
                        name: "General",
                        bundle_id: None,
                    },
                )
                .await?;
                use mxd::schema::news_articles::dsl as a;
                let posted = DateTime::<Utc>::from_timestamp(1000, 0)
                    .expect("valid timestamp")
                    .naive_utc();
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
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
    })?;

    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    handshake(&mut stream)?;

    let mut params = Vec::new();
    params.push((FieldId::NewsPath, b"General".as_ref()));
    let id_bytes = 1i32.to_be_bytes();
    params.push((FieldId::NewsArticleId, id_bytes.as_ref()));
    params.push((FieldId::NewsDataFlavor, b"text/plain".as_ref()));
    let payload = encode_params(&params)?;
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

#[test]
fn post_news_article_root() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        with_db(db, |conn| {
            Box::pin(async move {
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
    })?;

    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    handshake(&mut stream)?;

    let mut params = Vec::new();
    params.push((FieldId::NewsPath, b"General".as_ref()));
    params.push((FieldId::NewsTitle, b"Hello".as_ref()));
    let flag_bytes = 0i32.to_be_bytes();
    params.push((FieldId::NewsArticleFlags, flag_bytes.as_ref()));
    params.push((FieldId::NewsDataFlavor, b"text/plain".as_ref()));
    params.push((FieldId::NewsArticleData, b"hi".as_ref()));
    let payload = encode_params(&params)?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::PostNewsArticle.into(),
        id: 9,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;

    let mut hdr_buf = [0u8; 20];
    stream.read_exact(&mut hdr_buf)?;
    let hdr = FrameHeader::from_bytes(&hdr_buf);
    assert_eq!(hdr.error, 0);
    let mut payload = vec![0u8; hdr.data_size as usize];
    stream.read_exact(&mut payload)?;
    let params = decode_params(&payload)?;
    let mut id_found = false;
    for (id, data) in params {
        if id == FieldId::NewsArticleId {
            let arr: [u8; 4] = data.try_into().unwrap();
            assert_eq!(i32::from_be_bytes(arr), 1);
            id_found = true;
        }
    }
    assert!(id_found);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut conn = DbConnection::establish(server.db_path().to_str().unwrap()).await?;
        use mxd::schema::news_articles::dsl as a;
        let titles = a::news_articles
            .select(a::title)
            .load::<String>(&mut conn)
            .await?;
        assert_eq!(titles, vec!["Hello"]);
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}
