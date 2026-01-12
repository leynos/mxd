//! Integration tests for news category listing operations.
//!
//! Validates that the server correctly returns category hierarchies at various
//! paths (root, nested bundles, trailing slashes) and handles edge cases such
//! as invalid paths and empty databases.

use std::{
    convert::TryFrom,
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
use rstest::rstest;
use test_util::{
    AnyError,
    DatabaseUrl,
    handshake,
    login,
    setup_news_categories_nested_db,
    setup_news_categories_root_db,
};
mod common;

#[expect(clippy::expect_used, reason = "test helper: size and UTF-8 assertions")]
fn list_categories(port: u16, path: Option<&str>) -> Result<(FrameHeader, Vec<String>), AnyError> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(20)))?;
    handshake(&mut stream)?;
    login(&mut stream, "alice", "secret")?;
    let request_params = path
        .map(|p| vec![(FieldId::NewsPath, p.as_bytes())])
        .unwrap_or_default();
    let request_payload = encode_params(&request_params)?;
    let payload_size = u32::try_from(request_payload.len()).expect("payload fits in u32");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 1,
        error: 0,
        total_size: payload_size,
        data_size: payload_size,
    };
    let tx = Transaction {
        header,
        payload: request_payload,
    };
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
    let reply_params = decode_params(&reply_tx.payload)?;
    let names = reply_params
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::NewsCategory {
                Some(String::from_utf8(d).expect("category name must be valid UTF-8"))
            } else {
                None
            }
        })
        .collect();
    Ok((reply_tx.header, names))
}

/// rstest-decorated tests that need `panic_in_result_fn` suppression.
///
/// The rstest macro generates wrapper functions that cannot be individually annotated,
/// so module-level suppression is the narrowest possible scope.
mod rstest_tests {
    #![expect(
        clippy::panic_in_result_fn,
        reason = "rstest macro generates functions with assertions"
    )]

    use super::*;

    /// Tests that listing news categories at the root path (with or without explicit "/")
    /// returns all root-level bundles and categories.
    #[rstest]
    #[case(Some("/"))]
    #[case(None)]
    fn list_news_categories_root(#[case] path: Option<&str>) -> Result<(), AnyError> {
        let Some(server) = common::start_server_or_skip(setup_news_categories_root_db)? else {
            return Ok(());
        };

        let port = server.port();
        let (_, mut names) = list_categories(port, path)?;

        names.sort_unstable();
        let mut expected = vec!["Bundle", "General", "Updates"];
        expected.sort_unstable();

        assert_eq!(names, expected);
        Ok(())
    }

    /// Tests that requesting news categories at a nested bundle path returns only the categories
    /// within that sub-bundle, ignoring leading and trailing slashes.
    ///
    /// Sets up a nested bundle structure with a root bundle and a sub-bundle containing a single
    /// category. Sends a transaction requesting categories at the nested path and verifies that
    /// only the expected category is returned.
    #[rstest]
    #[case("Bundle/Sub")]
    #[case("/Bundle/Sub/")]
    fn list_news_categories_nested(#[case] path: &str) -> Result<(), AnyError> {
        let Some(server) = common::start_server_or_skip(setup_news_categories_nested_db)? else {
            return Ok(());
        };

        let port = server.port();
        let (_, names) = list_categories(port, Some(path))?;

        assert_eq!(names, vec!["Inside"]);
        Ok(())
    }
}

/// Tests that requesting news categories with an invalid path returns the expected unsupported path
/// error.
///
/// Sets up a database with a single category, sends a transaction with an invalid path parameter,
/// and asserts that the server responds with the `NEWS_ERR_PATH_UNSUPPORTED` error code.
///
/// # Returns
/// Returns `Ok(())` if the test passes; otherwise, returns an error if any step fails.
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[test]
fn list_news_categories_invalid_path() -> Result<(), AnyError> {
    use mxd::{db::create_user, models::NewUser, users::hash_password};

    let Some(server) = common::start_server_or_skip(|db: DatabaseUrl| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.as_str()).await?;
            apply_migrations(&mut conn, db.as_str()).await?;

            // Create test user for authentication
            let argon2 = argon2::Argon2::default();
            let hashed = hash_password(&argon2, "secret")?;
            let new_user = NewUser {
                username: "alice",
                password: &hashed,
            };
            create_user(&mut conn, &new_user).await?;

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
    })?
    else {
        return Ok(());
    };

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
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[test]
fn list_news_categories_empty() -> Result<(), AnyError> {
    use mxd::{db::create_user, models::NewUser, users::hash_password};

    let Some(server) = common::start_server_or_skip(|db: DatabaseUrl| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.as_str()).await?;
            apply_migrations(&mut conn, db.as_str()).await?;

            // Create test user for authentication
            let argon2 = argon2::Argon2::default();
            let hashed = hash_password(&argon2, "secret")?;
            let new_user = NewUser {
                username: "alice",
                password: &hashed,
            };
            create_user(&mut conn, &new_user).await?;

            Ok(())
        })
    })?
    else {
        return Ok(());
    };

    let port = server.port();
    let (_, names) = list_categories(port, None)?;
    assert!(names.is_empty());
    Ok(())
}
