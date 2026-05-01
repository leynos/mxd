//! Tests for presence registry and payload helpers.

use super::*;
use crate::{field_id::FieldId, transaction::decode_params};

fn snapshot(connection_id: u64, user_id: i32, display_name: &str) -> PresenceSnapshot {
    PresenceSnapshot {
        connection_id: OutboundConnectionId::new(connection_id),
        user_id,
        display_name: display_name.to_owned(),
        icon_id: 0,
        status_flags: 0,
    }
}

#[test]
fn field_300_encoding_matches_synhx_layout() {
    let mut snapshot = snapshot(4, 7, "alice");
    snapshot.icon_id = 9;
    snapshot.status_flags = 2;

    let encoded = snapshot
        .encode_user_name_with_info()
        .unwrap_or_else(|error| panic!("encode field 300: {error}"));

    assert_eq!(&encoded[0..2], &7u16.to_be_bytes());
    assert_eq!(&encoded[2..4], &9u16.to_be_bytes());
    assert_eq!(&encoded[4..6], &2u16.to_be_bytes());
    assert_eq!(&encoded[6..8], &5u16.to_be_bytes());
    assert_eq!(&encoded[8..], b"alice");
}

#[test]
fn field_300_encoding_rejects_oversized_values() {
    for snapshot in [
        snapshot(4, i32::from(u16::MAX) + 1, "alice"),
        snapshot(4, 7, &"a".repeat(usize::from(u16::MAX) + 1)),
    ] {
        let error = snapshot
            .encode_user_name_with_info()
            .expect_err("oversized field-300 value must be rejected");
        assert!(matches!(
            error,
            TransactionError::InvalidParamValue(FieldId::UserNameWithInfo)
        ));
    }
}

#[test]
fn registry_returns_sorted_snapshots() {
    let registry = PresenceRegistry::default();
    registry
        .upsert(snapshot(3, 9, "charlie"))
        .expect("insert charlie");
    registry
        .upsert(snapshot(1, 2, "alice"))
        .expect("insert alice");
    registry
        .upsert(snapshot(2, 2, "alice-2"))
        .expect("insert alice duplicate");

    let snapshots = registry.online_snapshots();

    assert_eq!(snapshots[0].connection_id, OutboundConnectionId::new(3));
    assert_eq!(snapshots[1].connection_id, OutboundConnectionId::new(1));
    assert_eq!(snapshots[2].connection_id, OutboundConnectionId::new(2));
}

fn registry_with_first_alice_upsert() -> (PresenceRegistry, PresenceUpsert) {
    let registry = PresenceRegistry::default();
    let first = registry
        .upsert(snapshot(1, 42, "alice"))
        .expect("insert alice");
    (registry, first)
}

#[test]
fn registry_assigns_unique_presence_ids_for_duplicate_account_logins() {
    let (registry, first) = registry_with_first_alice_upsert();
    let second = registry
        .upsert(snapshot(2, 42, "alice"))
        .expect("insert second login");

    assert_ne!(first.snapshot.user_id, second.snapshot.user_id);
    assert_eq!(registry.online_snapshots().len(), 2);
}

#[test]
fn registry_preserves_presence_id_when_session_updates() {
    let (registry, first) = registry_with_first_alice_upsert();
    let second = registry
        .upsert(snapshot(1, 42, "Alice A."))
        .expect("update snapshot");

    assert_eq!(first.snapshot.user_id, second.snapshot.user_id);
    assert_eq!(second.snapshot.display_name, "Alice A.");
}

#[test]
fn registry_remove_returns_remaining_peers() {
    let registry = PresenceRegistry::default();
    registry
        .upsert(snapshot(1, 1, "alice"))
        .expect("insert alice");
    registry
        .upsert(snapshot(3, 3, "charlie"))
        .expect("insert charlie");
    registry.upsert(snapshot(2, 2, "bob")).expect("insert bob");

    let removed = registry
        .remove(OutboundConnectionId::new(2))
        .unwrap_or_else(|| panic!("removed snapshot"));

    assert_eq!(removed.departed.user_id, 3);
    assert_eq!(
        removed.remaining_peer_ids,
        vec![OutboundConnectionId::new(1), OutboundConnectionId::new(3)]
    );
}

#[test]
fn notify_change_user_uses_server_initiated_transaction_id() {
    let mut snapshot = snapshot(1, 7, "alice");
    snapshot.icon_id = 4;
    snapshot.status_flags = 2;
    let transaction = build_notify_change_user(&snapshot)
        .unwrap_or_else(|error| panic!("encode notify change user: {error}"));
    assert_eq!(transaction.header.ty, 301);
    assert_eq!(transaction.header.is_reply, 0);
    assert_eq!(transaction.header.id, 0);
}

#[test]
fn notify_delete_user_uses_server_initiated_transaction_id() {
    let transaction = build_notify_delete_user(7)
        .unwrap_or_else(|error| panic!("encode notify delete user: {error}"));
    assert_eq!(transaction.header.ty, 302);
    assert_eq!(transaction.header.is_reply, 0);
    assert_eq!(transaction.header.id, 0);
}

#[test]
fn user_name_list_reply_contains_repeated_field_300_entries() {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetUserNameList.into(),
        id: 44,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    let reply =
        build_user_name_list_reply(&header, &[snapshot(1, 1, "alice"), snapshot(2, 2, "bob")])
            .unwrap_or_else(|error| panic!("build user list reply: {error}"));

    let params = decode_params(&reply.payload)
        .unwrap_or_else(|error| panic!("decode user list reply: {error}"));
    assert_eq!(params.len(), 2);
    assert!(
        params
            .iter()
            .all(|(field_id, _)| *field_id == FieldId::UserNameWithInfo)
    );
    assert_eq!(reply.header.is_reply, 1);
    assert_eq!(reply.header.id, 44);
}
