//! Tests for this module.
use rstest::rstest;

use super::*;
use crate::{
    commands::ERR_NOT_AUTHENTICATED,
    protocol::VERSION,
    transaction::{FrameHeader, Transaction, encode_params},
};

fn handshake(sub_version: u16) -> HandshakeMetadata {
    HandshakeMetadata {
        sub_protocol: u32::from_be_bytes(*b"HOTL"),
        version: VERSION,
        sub_version,
    }
}

fn reply_header(payload_len: usize) -> FrameHeader {
    let payload_len = match u32::try_from(payload_len) {
        Ok(payload_len) => payload_len,
        Err(err) => panic!("payload length fits in u32: {err}"),
    };
    FrameHeader {
        flags: 0,
        is_reply: 1,
        ty: TransactionType::Login.into(),
        id: 1,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    }
}

#[rstest]
fn classifies_synhx_from_handshake() {
    let compat = ClientCompatibility::from_handshake(&handshake(SYNHX_SUB_VERSION));
    assert_eq!(compat.kind(), ClientKind::SynHx);
}

#[rstest]
fn classifies_hotline_85_from_login_version() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    compat.record_login_version(151);
    assert_eq!(compat.kind(), ClientKind::Hotline85);
}

#[rstest]
fn classifies_hotline_19_from_login_version() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    compat.record_login_version(190);
    assert_eq!(compat.kind(), ClientKind::Hotline19);
}

#[rstest]
fn records_login_version_from_u16_payload() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    let payload = encode_params(&[(FieldId::Version, [0u8, 151u8])]).expect("payload encodes");

    compat
        .record_login_payload(&payload)
        .expect("record login version");

    assert_eq!(compat.login_version(), Some(151));
}

#[rstest]
fn records_login_version_from_u32_payload() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    let payload =
        encode_params(&[(FieldId::Version, [0u8, 0u8, 0u8, 190u8])]).expect("payload encodes");

    compat
        .record_login_payload(&payload)
        .expect("record login version");

    assert_eq!(compat.login_version(), Some(190));
}

fn assert_login_reply_augmentation(sub_version: u16, login_version: u16, expected_updated: bool) {
    let compat = ClientCompatibility::from_handshake(&handshake(sub_version));
    compat.record_login_version(login_version);
    let version_bytes = [(login_version >> 8) as u8, (login_version & 0x00ff) as u8];
    let payload = match encode_params(&[(FieldId::Version, version_bytes)]) {
        Ok(payload) => payload,
        Err(err) => panic!("payload encodes: {err}"),
    };
    let header = reply_header(payload.len());
    let mut reply = Transaction { header, payload };

    let updated = match compat.augment_login_reply(&mut reply) {
        Ok(updated) => updated,
        Err(err) => panic!("augment reply: {err}"),
    };

    assert_eq!(updated, expected_updated);
    let params = match decode_params(&reply.payload) {
        Ok(params) => params,
        Err(err) => panic!("decode reply params: {err}"),
    };
    let has_banner_id = params.iter().any(|(id, _)| *id == FieldId::BannerId);
    let has_server_name = params.iter().any(|(id, _)| *id == FieldId::ServerName);
    assert_eq!(has_banner_id, expected_updated);
    assert_eq!(has_server_name, expected_updated);
}

#[rstest]
fn augments_login_reply_when_required() { assert_login_reply_augmentation(0, 151, true); }

#[rstest]
fn does_not_augment_login_reply_for_synhx() {
    assert_login_reply_augmentation(SYNHX_SUB_VERSION, 190, false);
}

#[rstest]
fn classifies_unknown_for_older_login_versions() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    compat.record_login_version(100);
    assert_eq!(compat.kind(), ClientKind::Unknown);
}

#[rstest]
fn does_not_augment_login_reply_for_unknown_client_kind() {
    assert_login_reply_augmentation(0, 100, false);
}

#[rstest]
fn does_not_augment_failed_login_reply_for_hotline_clients() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    compat.record_login_version(190);
    #[expect(
        clippy::big_endian_bytes,
        reason = "network protocol uses big-endian integers"
    )]
    let payload = encode_params(&[(FieldId::Version, 190u16.to_be_bytes().as_ref())])
        .expect("payload encodes");
    let payload_len = u32::try_from(payload.len()).expect("payload length fits in u32");
    let header = FrameHeader {
        flags: 0,
        is_reply: 1,
        ty: TransactionType::Login.into(),
        id: 1,
        error: ERR_NOT_AUTHENTICATED,
        total_size: payload_len,
        data_size: payload_len,
    };
    let mut reply = Transaction {
        header,
        payload: payload.clone(),
    };

    let updated = compat
        .augment_login_reply(&mut reply)
        .expect("augment reply");

    assert!(!updated, "failed login replies must not be augmented");
    assert_eq!(reply.payload, payload);
}

#[rstest]
fn records_u16_max_version_without_sentinel_collision() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    compat.record_login_version(u16::MAX);
    assert_eq!(compat.login_version(), Some(u16::MAX));
}

#[rstest]
fn synhx_classification_takes_precedence_over_login_version() {
    let compat = ClientCompatibility::from_handshake(&handshake(SYNHX_SUB_VERSION));
    compat.record_login_version(190);
    assert_eq!(compat.kind(), ClientKind::SynHx);
    assert!(!compat.should_include_login_extras());
}

#[rstest]
#[case(150, false)]
#[case(151, true)]
#[case(189, true)]
#[case(190, true)]
fn login_extras_gate_matches_version_boundaries(
    #[case] login_version: u16,
    #[case] should_include: bool,
) {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    compat.record_login_version(login_version);

    assert_eq!(compat.should_include_login_extras(), should_include);
}

#[rstest]
fn augment_login_reply_is_idempotent_when_extras_exist() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    compat.record_login_version(190);
    #[expect(
        clippy::big_endian_bytes,
        reason = "network protocol uses big-endian integers"
    )]
    let payload = encode_params(&[
        (FieldId::Version, 190u16.to_be_bytes().as_ref()),
        (FieldId::BannerId, 0i32.to_be_bytes().as_ref()),
        (FieldId::ServerName, b"mxd".as_ref()),
    ])
    .expect("payload encodes");
    let original_payload = payload.clone();
    let header = reply_header(payload.len());
    let mut reply = Transaction { header, payload };

    let updated = compat
        .augment_login_reply(&mut reply)
        .expect("augment reply");

    assert!(!updated, "extras are already present");
    assert_eq!(reply.payload, original_payload);
}

#[rstest]
fn record_login_payload_ignores_unparseable_version_lengths() {
    let compat = ClientCompatibility::from_handshake(&handshake(0));
    let payload = encode_params(&[(FieldId::Version, vec![1u8, 2u8, 3u8].as_slice())])
        .expect("payload encodes");

    compat
        .record_login_payload(&payload)
        .expect("record login payload");

    assert_eq!(compat.login_version(), None);
    assert_eq!(compat.kind(), ClientKind::Unknown);
}
