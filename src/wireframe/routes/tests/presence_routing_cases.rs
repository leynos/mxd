//! Presence-specific routing tests.

use rstest::rstest;
use test_util::{AnyError, build_test_db, setup_files_db};

use super::helpers::{RouteTestContext, decode_reply_params, find_string, runtime};
use crate::{field_id::FieldId, privileges::Privileges, transaction_type::TransactionType};

fn decode_user_name_with_info(payload: &[u8]) -> Result<(u16, u16, u16, String), AnyError> {
    let user_id = u16::from_be_bytes(payload[0..2].try_into()?);
    let icon_id = u16::from_be_bytes(payload[2..4].try_into()?);
    let flags = u16::from_be_bytes(payload[4..6].try_into()?);
    let name_len = usize::from(u16::from_be_bytes(payload[6..8].try_into()?));
    let name = std::str::from_utf8(&payload[8..8 + name_len])?.to_owned();
    Ok((user_id, icon_id, flags, name))
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_user_name_list_returns_online_snapshot() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;

    let login = rt.block_on(ctx.send(
        TransactionType::Login,
        20,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    ))?;
    assert_eq!(login.header.error, 0);

    let reply = rt.block_on(ctx.send(TransactionType::GetUserNameList, 21, &[]))?;
    assert_eq!(reply.header.error, 0);
    let params = decode_reply_params(&reply)?;
    let user_entry = params
        .iter()
        .find(|(field_id, _)| *field_id == FieldId::UserNameWithInfo)
        .ok_or_else(|| anyhow::anyhow!("missing field 300 in user list reply"))?;
    let (user_id, icon_id, flags, display_name) = decode_user_name_with_info(&user_entry.1)?;
    assert_eq!(user_id, 1);
    assert_eq!(icon_id, 0);
    assert_eq!(flags, 0);
    assert_eq!(display_name, "alice");
    Ok(())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_client_info_text_returns_name_and_blank_info() -> Result<(), AnyError>
{
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    let login = rt.block_on(ctx.send(
        TransactionType::Login,
        22,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    ))?;
    assert_eq!(login.header.error, 0);

    let target_user_id = 1u16.to_be_bytes();
    let reply = rt.block_on(ctx.send(
        TransactionType::GetClientInfoText,
        23,
        &[(FieldId::UserId, target_user_id.as_ref())],
    ))?;
    assert_eq!(reply.header.error, 0);
    let params = decode_reply_params(&reply)?;
    assert_eq!(find_string(&params, FieldId::Name)?, "alice");
    assert_eq!(find_string(&params, FieldId::Data)?, "");
    Ok(())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_client_info_text_unknown_user_uses_internal_error()
-> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    ctx.authenticate_with_privileges(1, Privileges::GET_CLIENT_INFO | Privileges::NO_AGREEMENT);

    let unknown_user_id = 42u32.to_be_bytes();
    let reply = rt.block_on(ctx.send(
        TransactionType::GetClientInfoText,
        24,
        &[(FieldId::UserId, unknown_user_id.as_ref())],
    ))?;

    assert_eq!(reply.header.error, crate::commands::ERR_INTERNAL_SERVER);
    assert!(reply.payload.is_empty());
    Ok(())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_client_info_text_accepts_mixed_width_user_id_encodings()
-> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    let login = rt.block_on(ctx.send(
        TransactionType::Login,
        25,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    ))?;
    assert_eq!(login.header.error, 0);

    let target_user_id_u16 = 1u16.to_be_bytes();
    let reply_16 = rt.block_on(ctx.send(
        TransactionType::GetClientInfoText,
        26,
        &[(FieldId::UserId, target_user_id_u16.as_ref())],
    ))?;
    assert_eq!(reply_16.header.error, 0);
    let params_16 = decode_reply_params(&reply_16)?;
    assert_eq!(find_string(&params_16, FieldId::Name)?, "alice");
    assert_eq!(find_string(&params_16, FieldId::Data)?, "");

    let target_user_id_u32 = 1u32.to_be_bytes();
    let reply_32 = rt.block_on(ctx.send(
        TransactionType::GetClientInfoText,
        27,
        &[(FieldId::UserId, target_user_id_u32.as_ref())],
    ))?;
    assert_eq!(reply_32.header.error, 0);
    let params_32 = decode_reply_params(&reply_32)?;
    assert_eq!(find_string(&params_32, FieldId::Name)?, "alice");
    assert_eq!(find_string(&params_32, FieldId::Data)?, "");
    Ok(())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_set_client_user_info_updates_future_user_list_views()
-> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;
    let login = rt.block_on(ctx.send(
        TransactionType::Login,
        24,
        &[(FieldId::Login, b"alice"), (FieldId::Password, b"secret")],
    ))?;
    assert_eq!(login.header.error, 0);

    let icon_id = 9u16.to_be_bytes();
    let options = 4u16.to_be_bytes();
    let update = rt.block_on(ctx.send(
        TransactionType::SetClientUserInfo,
        25,
        &[
            (FieldId::Name, b"Alice A."),
            (FieldId::IconId, icon_id.as_ref()),
            (FieldId::Options, options.as_ref()),
            (FieldId::AutoResponse, b"back soon"),
        ],
    ))?;
    assert_eq!(update.header.error, 0);
    assert_eq!(ctx.session.display_name, "Alice A.");
    assert_eq!(ctx.session.icon_id, 9);
    assert_eq!(
        ctx.session.connection_flags,
        crate::connection_flags::ConnectionFlags::AUTOMATIC_RESPONSE
    );
    assert_eq!(ctx.session.auto_response.as_deref(), Some("back soon"));

    let user_list = rt.block_on(ctx.send(TransactionType::GetUserNameList, 26, &[]))?;
    let params = decode_reply_params(&user_list)?;
    let user_entry = params
        .iter()
        .find(|(field_id, _)| *field_id == FieldId::UserNameWithInfo)
        .ok_or_else(|| anyhow::anyhow!("missing field 300 in updated user list reply"))?;
    let (_, listed_icon_id, _, display_name) = decode_user_name_with_info(&user_entry.1)?;
    assert_eq!(listed_icon_id, 9);
    assert_eq!(display_name, "Alice A.");
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[rstest]
fn process_transaction_bytes_user_name_list_requires_online_session() -> Result<(), AnyError> {
    let rt = runtime()?;
    let Some(test_db) = build_test_db(&rt, setup_files_db)? else {
        return Ok(());
    };
    let mut ctx = RouteTestContext::new(test_db.pool())?;

    let reply = rt.block_on(ctx.send(TransactionType::GetUserNameList, 27, &[]))?;

    assert_eq!(reply.header.error, crate::commands::ERR_NOT_AUTHENTICATED);
    Ok(())
}
