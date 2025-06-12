use std::io::{Read, Write};
use std::net::TcpStream;

use diesel::prelude::*;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;
use mxd::commands::NEWS_ERR_PATH_UNSUPPORTED;
use mxd::db::apply_migrations;
use mxd::db::{DbConnection, create_bundle, create_category};
use mxd::field_id::FieldId;
use mxd::models::NewCategory;
use mxd::transaction::encode_params;
use mxd::transaction::{FrameHeader, Transaction, decode_params};
use mxd::transaction_type::TransactionType;
use test_util::{TestServer, handshake};

fn list_categories(
    port: u16,
    path: Option<&str>,
) -> Result<(FrameHeader, Vec<String>), Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(1)))?;
    handshake(&mut stream)?;
    let params = path
        .map(|p| vec![(FieldId::NewsPath, p.as_bytes())])
        .unwrap_or_default();
    let payload = encode_params(&params)?;
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
    let names = params
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::NewsCategory {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect();
    Ok((reply_tx.header, names))
}

#[test]
fn list_news_categories_root() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db).await?;
            apply_migrations(&mut conn, db).await?;
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
    let (_, names) = list_categories(port, Some("/"))?;
    assert_eq!(names, vec!["Bundle", "General", "Updates"]);
    Ok(())
}

#[test]
fn list_news_categories_no_path() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db).await?;
            apply_migrations(&mut conn, db).await?;
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
    let (_, names) = list_categories(port, None)?;
    assert_eq!(names, vec!["Bundle", "General", "Updates"]);
    Ok(())
}

#[test]
fn list_news_categories_invalid_path() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db).await?;
            apply_migrations(&mut conn, db).await?;
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
    let (hdr, _) = list_categories(port, Some("some/path"))?;
    assert_eq!(hdr.error, NEWS_ERR_PATH_UNSUPPORTED);
    Ok(())
}
#[test]
fn list_news_categories_empty() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db).await?;
            apply_migrations(&mut conn, db).await?;
            Ok(())
        })
    })?;

    let port = server.port();
    let (_, names) = list_categories(port, None)?;
    assert!(names.is_empty());
    Ok(())
}

#[test]
fn list_news_categories_nested() -> Result<(), Box<dyn std::error::Error>> {
    use mxd::models::{NewBundle, NewCategory};
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db).await?;
            apply_migrations(&mut conn, db).await?;
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
    let (_, names) = list_categories(port, Some("Bundle/Sub"))?;
    assert_eq!(names, vec!["Inside"]);
    Ok(())
}
