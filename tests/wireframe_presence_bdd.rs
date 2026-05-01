//! Behavioural tests for session-scoped presence flows through the router.

#![expect(clippy::big_endian_bytes, reason = "protocol fixtures use big-endian")]

use anyhow::anyhow;
use mxd::{field_id::FieldId, transaction::decode_params, transaction_type::TransactionType};
use rstest::fixture;
use rstest_bdd::assert_step_ok;
use rstest_bdd_macros::{given, scenarios, then, when};
use test_util::AnyError;

#[path = "wireframe_presence_bdd/wireframe_presence_support.rs"]
mod wireframe_presence_support;

use wireframe_presence_support::{PresenceWorld, RequestSpec};

fn decode_protocol_u32(bytes: &[u8]) -> Result<u32, AnyError> {
    match bytes.len() {
        2 => Ok(u32::from(u16::from_be_bytes(bytes.try_into()?))),
        4 => Ok(u32::from_be_bytes(bytes.try_into()?)),
        _ => Err(anyhow!("unexpected integer width {}", bytes.len())),
    }
}

fn decode_user_name_with_info(bytes: &[u8]) -> Result<(u16, String), AnyError> {
    let user_id = decode_u16_slice(bytes, 0..2)?;
    let name_len = usize::from(decode_u16_slice(bytes, 6..8)?);
    let name_bytes = bytes
        .get(8..8 + name_len)
        .ok_or_else(|| anyhow!("field 300 name exceeds payload length"))?;
    let name = std::str::from_utf8(name_bytes)?.to_owned();
    Ok((user_id, name))
}

fn decode_u16_slice(bytes: &[u8], range: std::ops::Range<usize>) -> Result<u16, AnyError> {
    let slice = bytes
        .get(range)
        .ok_or_else(|| anyhow!("payload shorter than expected"))?;
    Ok(u16::from_be_bytes(slice.try_into()?))
}

fn find_param(params: &[(FieldId, Vec<u8>)], field_id: FieldId) -> Result<&[u8], AnyError> {
    params
        .iter()
        .find(|(candidate, _)| *candidate == field_id)
        .map(|(_, bytes)| bytes.as_slice())
        .ok_or_else(|| anyhow!("missing {field_id}"))
}

type FieldParam = (FieldId, Vec<u8>);

fn read_u32_param(params: &[(FieldId, Vec<u8>)], field_id: FieldId) -> Result<u32, AnyError> {
    decode_protocol_u32(find_param(params, field_id)?)
}

fn read_string_param(params: &[(FieldId, Vec<u8>)], field_id: FieldId) -> Result<String, AnyError> {
    Ok(std::str::from_utf8(find_param(params, field_id)?)?.to_owned())
}

#[fixture]
#[rustfmt::skip]
fn world() -> PresenceWorld {
    PresenceWorld::new()
}

#[given("a wireframe server with two presence test users")]
fn given_server(world: &PresenceWorld) -> Result<(), AnyError> { world.setup_db() }

#[given("client \"{label}\" is connected and logged in as \"{username}\"")]
fn given_client_logged_in(
    world: &PresenceWorld,
    label: String,
    username: String,
) -> Result<(), AnyError> {
    if world.is_skipped() {
        return Ok(());
    }
    world.send(
        &label,
        RequestSpec {
            ty: TransactionType::Login,
            id: 10,
            params: &[
                (FieldId::Login, username.as_bytes()),
                (FieldId::Password, b"secret"),
            ],
        },
    )?;
    world.with_last_transaction(|transaction| {
        if transaction.header.error != 0 {
            return Err(anyhow!(
                "expected successful login reply, got error {}",
                transaction.header.error
            ));
        }
        Ok(())
    })
}

#[when("client \"{label}\" requests the user name list")]
fn when_request_user_list(world: &PresenceWorld, label: String) -> Result<(), AnyError> {
    if world.is_skipped() {
        return Ok(());
    }
    world.send(
        &label,
        RequestSpec {
            ty: TransactionType::GetUserNameList,
            id: 11,
            params: &[],
        },
    )
}

#[when("client \"{label}\" updates their user info to name \"{name}\" and icon {icon_id}")]
fn when_update_user_info(
    world: &PresenceWorld,
    label: String,
    name: String,
    icon_id: u16,
) -> Result<(), AnyError> {
    if world.is_skipped() {
        return Ok(());
    }
    let icon_bytes = icon_id.to_be_bytes();
    world.send(
        &label,
        RequestSpec {
            ty: TransactionType::SetClientUserInfo,
            id: 12,
            params: &[
                (FieldId::Name, name.as_bytes()),
                (FieldId::IconId, icon_bytes.as_ref()),
            ],
        },
    )
}

#[when("client \"{label}\" disconnects")]
fn when_disconnect(world: &PresenceWorld, label: String) -> Result<(), AnyError> {
    if world.is_skipped() {
        return Ok(());
    }
    world.disconnect(&label)
}

struct NotifyChangeUserExpected {
    user_id: u32,
    name: String,
    icon_id: u32,
}

fn verify_notify_change_user_fields(
    params: &[FieldParam],
    expected: &NotifyChangeUserExpected,
) -> Result<(), AnyError> {
    let actual_user_id = read_u32_param(params, FieldId::UserId)?;
    let actual_icon_id = read_u32_param(params, FieldId::IconId)?;
    let actual_name = read_string_param(params, FieldId::Name)?;
    if actual_user_id != expected.user_id {
        return Err(anyhow!(
            "expected user id {}, got {actual_user_id}",
            expected.user_id
        ));
    }
    if actual_icon_id != expected.icon_id {
        return Err(anyhow!(
            "expected icon id {}, got {actual_icon_id}",
            expected.icon_id
        ));
    }
    if actual_name != expected.name {
        return Err(anyhow!(
            "expected name {:?}, got {actual_name:?}",
            expected.name
        ));
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "bdd step signature mirrors feature-file placeholders; cannot be reduced"
)]
#[then(
    "client \"{label}\" receives a notify change user for user {user_id} with name \"{name}\" and \
     icon {icon_id}"
)]
fn then_notify_change_user(
    world: &PresenceWorld,
    label: String,
    user_id: u32,
    name: String,
    icon_id: u32,
) -> Result<(), AnyError> {
    if world.is_skipped() {
        return Ok(());
    }
    let notification = world.observe_notification(&label)?;
    if notification.header.ty != u16::from(TransactionType::NotifyChangeUser) {
        return Err(anyhow!(
            "expected notify change user, got transaction {}",
            notification.header.ty
        ));
    }
    let params = assert_step_ok!(decode_params(&notification.payload).map_err(|e| e.to_string()));
    verify_notify_change_user_fields(
        &params,
        &NotifyChangeUserExpected {
            user_id,
            name,
            icon_id,
        },
    )
}

#[then("client \"{label}\" receives a notify delete user for user {user_id}")]
fn then_notify_delete_user(
    world: &PresenceWorld,
    label: String,
    user_id: u32,
) -> Result<(), AnyError> {
    if world.is_skipped() {
        return Ok(());
    }
    let notification = world.observe_notification(&label)?;
    if notification.header.ty != u16::from(TransactionType::NotifyDeleteUser) {
        return Err(anyhow!(
            "expected notify delete user, got transaction {}",
            notification.header.ty
        ));
    }
    let params = assert_step_ok!(decode_params(&notification.payload).map_err(|e| e.to_string()));
    let actual_user_id = read_u32_param(&params, FieldId::UserId)?;
    if actual_user_id != user_id {
        return Err(anyhow!(
            "expected deleted user id {user_id}, got {actual_user_id}"
        ));
    }
    Ok(())
}

#[then("the reply lists online users \"{first}\" and \"{second}\"")]
fn then_reply_lists_online_users(
    world: &PresenceWorld,
    first: String,
    second: String,
) -> Result<(), AnyError> {
    if world.is_skipped() {
        return Ok(());
    }
    world.with_last_transaction(|transaction| {
        if transaction.header.error != 0 {
            return Err(anyhow!(
                "expected user list reply success, got error {}",
                transaction.header.error
            ));
        }
        let params =
            assert_step_ok!(decode_params(&transaction.payload).map_err(|e| e.to_string()));
        let mut users = params
            .iter()
            .filter(|(field_id, _)| *field_id == FieldId::UserNameWithInfo)
            .map(|(_, payload)| decode_user_name_with_info(payload))
            .collect::<Result<Vec<_>, _>>()?;
        users.sort_by_key(|(user_id, _)| *user_id);
        let expected_users = vec![(1, first), (2, second)];
        if users != expected_users {
            return Err(anyhow!(
                "expected online users {expected_users:?}, got {users:?}"
            ));
        }
        Ok(())
    })
}

scenarios!(
    "tests/features/wireframe_presence.feature",
    fixtures = [world: PresenceWorld]
);
