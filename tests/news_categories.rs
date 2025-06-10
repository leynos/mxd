use std::io::{Read, Write};
use std::net::TcpStream;

use diesel::prelude::*;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;
use mxd::commands::NEWS_ERR_PATH_UNSUPPORTED;
use mxd::db::{DbConnection, create_bundle, create_category, run_migrations};
use mxd::field_id::FieldId;
use mxd::models::NewCategory;
use mxd::transaction::encode_params;
use mxd::transaction::{FrameHeader, Transaction, decode_params};
use mxd::transaction_type::TransactionType;
use test_util::TestServer;

#[test]
fn list_news_categories_root() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await?;
            run_migrations(&mut conn).await?;
            create_bundle(
                &mut conn,
                &mxd::models::NewBundle {
                    parent_bundle_id: None,
                    name: "Bundle",
                },
            )
            .await?;
            create_category(
                &mut conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            create_category(
                &mut conn,
                &NewCategory {
                    name: "Updates",
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

    let params = vec![(FieldId::NewsPath, b"/".as_ref())];
    let payload = encode_params(&params)?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 3,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    let frame = tx.to_bytes();
    stream.write_all(&frame)?;

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
    let names = params
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::NewsCategory {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Bundle", "General", "Updates"]);
    Ok(())
}

#[test]
fn list_news_categories_no_path() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await?;
            run_migrations(&mut conn).await?;
            create_bundle(
                &mut conn,
                &mxd::models::NewBundle {
                    parent_bundle_id: None,
                    name: "Bundle",
                },
            )
            .await?;
            create_category(
                &mut conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            create_category(
                &mut conn,
                &NewCategory {
                    name: "Updates",
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

    let payload = encode_params(&[])?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 1,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    let frame = tx.to_bytes();
    stream.write_all(&frame)?;

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
    let names = params
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::NewsCategory {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Bundle", "General", "Updates"]);
    Ok(())
}

#[test]
fn list_news_categories_invalid_path() -> Result<(), Box<dyn std::error::Error>> {
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

    let params = vec![(FieldId::NewsPath, b"some/path".as_ref())];
    let payload = encode_params(&params)?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 2,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    let frame = tx.to_bytes();
    stream.write_all(&frame)?;

    let mut hdr_buf = [0u8; 20];
    stream.read_exact(&mut hdr_buf)?;
    let hdr = FrameHeader::from_bytes(&hdr_buf);
    assert_eq!(hdr.error, NEWS_ERR_PATH_UNSUPPORTED);
    Ok(())
}
#[test]
fn list_news_categories_empty() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await?;
            run_migrations(&mut conn).await?;
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

    let payload = encode_params(&[])?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 4,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    let frame = tx.to_bytes();
    stream.write_all(&frame)?;

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
            if id == FieldId::NewsCategory {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect();
    assert!(names.is_empty());
    Ok(())
}

#[test]
fn list_news_categories_nested() -> Result<(), Box<dyn std::error::Error>> {
    use mxd::models::{NewBundle, NewCategory};
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await?;
            run_migrations(&mut conn).await?;
            use mxd::schema::news_bundles::dsl as b;

            create_bundle(
                &mut conn,
                &NewBundle {
                    parent_bundle_id: None,
                    name: "Bundle",
                },
            )
            .await?;
            let root_id: i32 = b::news_bundles
                .filter(b::name.eq("Bundle"))
                .filter(b::parent_bundle_id.is_null())
                .select(b::id)
                .first(&mut conn)
                .await?;

            create_bundle(
                &mut conn,
                &NewBundle {
                    parent_bundle_id: Some(root_id),
                    name: "Sub",
                },
            )
            .await?;
            let sub_id: i32 = b::news_bundles
                .filter(b::name.eq("Sub"))
                .filter(b::parent_bundle_id.eq(root_id))
                .select(b::id)
                .first(&mut conn)
                .await?;

            create_category(
                &mut conn,
                &NewCategory {
                    name: "Inside",
                    bundle_id: Some(sub_id),
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

    let params = vec![(FieldId::NewsPath, b"Bundle/Sub".as_ref())];
    let payload = encode_params(&params)?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 5,
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
            if id == FieldId::NewsCategory {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, vec!["Inside"]);
    Ok(())
}
