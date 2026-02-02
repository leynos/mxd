//! Unit tests covering successful transaction routing.

use rstest::rstest;
use test_util::{
    AnyError,
    build_test_db,
    setup_files_db,
    setup_news_categories_root_db,
    setup_news_db,
};

use super::helpers::{
    RouteTestContext,
    collect_strings,
    decode_reply_params,
    find_i32,
    find_string,
    runtime,
};
use crate::{field_id::FieldId, privileges::Privileges, transaction_type::TransactionType};

fn xor_bytes(data: &[u8]) -> Vec<u8> { data.iter().map(|byte| byte ^ 0xFF).collect() }

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_login_success() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;

    let reply = rt.block_on(ctx.send(
        TransactionType::Login,
        1,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    ))?;

    assert_eq!(reply.header.error, 0);
    assert_eq!(reply.header.id, 1);
    assert!(ctx.session.user_id.is_some());
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_login_success_with_xor_password() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    let encoded_login = xor_bytes(b"alice");
    let encoded_password = xor_bytes(b"secret");

    let reply = rt.block_on(ctx.send(
        TransactionType::Login,
        9,
        &[
            (FieldId::Login, encoded_login.as_slice()),
            (FieldId::Password, encoded_password.as_slice()),
        ],
    ))?;

    assert_eq!(reply.header.error, 0);
    assert!(ctx.compat().is_enabled());
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_enables_xor_from_message_field() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    let encoded_message = xor_bytes(b"hello");

    let reply = rt.block_on(ctx.send(
        TransactionType::Other(900),
        10,
        &[(FieldId::Data, encoded_message.as_slice())],
    ))?;

    assert_eq!(reply.header.error, crate::commands::ERR_INTERNAL_SERVER);
    assert!(ctx.compat().is_enabled());
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_file_list_success() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;

    let login = rt.block_on(ctx.send(
        TransactionType::Login,
        1,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    ))?;
    assert_eq!(login.header.error, 0);

    let reply = rt.block_on(ctx.send(TransactionType::GetFileNameList, 2, &[]))?;
    assert_eq!(reply.header.error, 0);
    assert_eq!(reply.header.id, 2);

    let params = decode_reply_params(&reply)?;
    let names = collect_strings(&params, FieldId::FileName)?;
    assert_eq!(names, vec!["fileA.txt", "fileC.txt"]);
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_news_category_list_success() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_news_categories_root_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    ctx.authenticate(1);

    let reply = rt.block_on(ctx.send(TransactionType::NewsCategoryNameList, 3, &[]))?;
    assert_eq!(reply.header.error, 0);
    assert_eq!(reply.header.id, 3);

    let params = decode_reply_params(&reply)?;
    let mut names = collect_strings(&params, FieldId::NewsCategory)?;
    names.sort_unstable();
    let expected = vec!["Bundle", "General", "Updates"];
    assert_eq!(names, expected);
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_news_article_list_success() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_news_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    ctx.authenticate(1);

    let reply = rt.block_on(ctx.send(
        TransactionType::NewsArticleNameList,
        4,
        &[(FieldId::NewsPath, b"General")],
    ))?;
    assert_eq!(reply.header.error, 0);
    assert_eq!(reply.header.id, 4);

    let params = decode_reply_params(&reply)?;
    let names = collect_strings(&params, FieldId::NewsArticle)?;
    assert_eq!(names, vec!["First", "Second"]);
    Ok(())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_news_article_data_success() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_news_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    ctx.authenticate(1);

    let article_id = 1i32.to_be_bytes();
    let reply = rt.block_on(ctx.send(
        TransactionType::NewsArticleData,
        5,
        &[
            (FieldId::NewsPath, b"General"),
            (FieldId::NewsArticleId, article_id.as_ref()),
        ],
    ))?;
    assert_eq!(reply.header.error, 0);
    assert_eq!(reply.header.id, 5);

    let params = decode_reply_params(&reply)?;
    assert_eq!(find_string(&params, FieldId::NewsTitle)?, "First");
    assert_eq!(find_string(&params, FieldId::NewsArticleData)?, "a");
    Ok(())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_post_news_article_success() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_news_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    // Authenticate with default user privileges (includes NEWS_POST_ARTICLE)
    ctx.authenticate_with_privileges(1, Privileges::default_user());

    let flags = 0i32.to_be_bytes();
    let reply = rt.block_on(ctx.send(
        TransactionType::PostNewsArticle,
        6,
        &[
            (FieldId::NewsPath, b"General"),
            (FieldId::NewsTitle, b"Third"),
            (FieldId::NewsArticleFlags, flags.as_ref()),
            (FieldId::NewsDataFlavor, b"text/plain"),
            (FieldId::NewsArticleData, b"hello"),
        ],
    ))?;
    assert_eq!(reply.header.error, 0);
    assert_eq!(reply.header.id, 6);

    let params = decode_reply_params(&reply)?;
    assert!(find_i32(&params, FieldId::NewsArticleId)? > 0);

    let list_reply = rt.block_on(ctx.send(
        TransactionType::NewsArticleNameList,
        7,
        &[(FieldId::NewsPath, b"General")],
    ))?;
    let list_params = decode_reply_params(&list_reply)?;
    let names = collect_strings(&list_params, FieldId::NewsArticle)?;
    assert!(names.contains(&"Third"));
    Ok(())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_post_news_article_success_with_xor_data() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_news_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    ctx.authenticate_with_privileges(1, Privileges::default_user());

    let flags = 0i32.to_be_bytes();
    let encoded_path = xor_bytes(b"General");
    let encoded_title = xor_bytes(b"XorTitle");
    let encoded_flavor = xor_bytes(b"text/plain");
    let encoded_data = xor_bytes(b"xor body");

    let reply = rt.block_on(ctx.send(
        TransactionType::PostNewsArticle,
        8,
        &[
            (FieldId::NewsPath, encoded_path.as_slice()),
            (FieldId::NewsTitle, encoded_title.as_slice()),
            (FieldId::NewsArticleFlags, flags.as_ref()),
            (FieldId::NewsDataFlavor, encoded_flavor.as_slice()),
            (FieldId::NewsArticleData, encoded_data.as_slice()),
        ],
    ))?;
    assert_eq!(reply.header.error, 0);
    assert!(ctx.compat().is_enabled());
    Ok(())
}
