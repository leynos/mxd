use std::{
    io::{Read, Write},
    net::TcpStream,
};

use diesel_async::AsyncConnection;
use mxd::{
    commands::NEWS_ERR_PATH_UNSUPPORTED,
    db::{DbConnection, apply_migrations, create_category},
    field_id::FieldId,
    models::NewCategory,
    transaction::{FrameHeader, Transaction, decode_params, encode_params},
    transaction_type::TransactionType,
};
use test_util::{
    TestServer,
    handshake,
    setup_news_categories_nested_db,
    setup_news_categories_root_db,
};

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

/// Tests that listing news categories at the root path returns all root-level bundles and
/// categories.
///
/// Sets up a test server with one bundle ("Bundle") and two categories ("General", "Updates") at
/// the root level. Sends a transaction requesting news categories at the root path ("/") and
/// verifies that the response contains all expected category names.
///
/// # Errors
///
/// Returns an error if the test server setup, TCP communication, or protocol validation fails.
#[test]
fn list_news_categories_root() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", setup_news_categories_root_db)?;

    let port = server.port();
    let (_, mut names) = list_categories(port, Some("/"))?;
    names.sort();
    assert_eq!(names, vec!["Bundle", "General", "Updates"]);
    Ok(())
}

/// Tests that listing news categories with no path parameter returns all root-level bundles and
/// categories.
///
/// Sets up a database with one bundle ("Bundle") and two categories ("General", "Updates") not
/// associated with any bundle. Sends a transaction request without a path parameter and verifies
/// that the response contains all three names.
///
/// # Errors
///
/// Returns an error if the test server setup, database operations, TCP communication, or protocol
/// decoding fails.
///
/// # Examples
///
/// ```
/// list_news_categories_no_path().unwrap(); 
/// ```
#[test]
fn list_news_categories_no_path() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", setup_news_categories_root_db)?;

    let port = server.port();
    let (_, mut names) = list_categories(port, None)?;
    names.sort();
    assert_eq!(names, vec!["Bundle", "General", "Updates"]);
    Ok(())
}

/// Tests that requesting news categories with an invalid path returns the expected unsupported path
/// error.
///
/// Sets up a database with a single category, sends a transaction with an invalid path parameter,
/// and asserts that the server responds with the `NEWS_ERR_PATH_UNSUPPORTED` error code.
///
/// # Returns
/// Returns `Ok(())` if the test passes; otherwise, returns an error if any step fails.
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
/// Tests that requesting a list of news categories from an empty database returns no categories.
///
/// This test sets up a test server with an empty database, performs a TCP handshake,
/// sends a news category listing transaction, and asserts that the response contains no category
/// names.
///
/// # Errors
///
/// Returns an error if the test server setup, TCP communication, or protocol operations fail.
///
/// # Examples
///
/// ```
/// list_news_categories_empty().unwrap(); 
/// ```
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

/// Tests that requesting news categories at a nested bundle path returns only the categories within
/// that sub-bundle.
///
/// Sets up a nested bundle structure with a root bundle and a sub-bundle containing a single
/// category. Sends a transaction requesting categories at the nested path and verifies that only
/// the expected category is returned.
///
/// # Errors
///
/// Returns an error if the test server setup, database operations, TCP communication, or protocol
/// decoding fails.
#[test]
fn list_news_categories_nested() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", setup_news_categories_nested_db)?;

    let port = server.port();
    let (_, names) = list_categories(port, Some("Bundle/Sub"))?;
    assert_eq!(names, vec!["Inside"]);
    Ok(())
}
