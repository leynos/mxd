//! Runtime presence state and payload helpers.
//!
//! This module keeps the server's online-user view in one place so handlers can
//! build consistent `300`, `301`, `302`, and `303` transactions without
//! re-encoding presence state in multiple branches.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use crate::{
    field_id::FieldId,
    header_util::reply_header,
    server::outbound::OutboundConnectionId,
    transaction::{FrameHeader, Transaction, TransactionError, encode_params},
    transaction_type::TransactionType,
};

/// A connection's visibility within the presence lifecycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionPhase {
    /// The connection has not authenticated yet.
    #[default]
    Unauthenticated,
    /// The connection authenticated but must still complete agreement flow.
    PendingAgreement,
    /// The connection is fully online and may appear in presence lists.
    Online,
}

/// Presence data shared with other clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceSnapshot {
    /// Outbound connection identifier used for push delivery.
    pub connection_id: OutboundConnectionId,
    /// Logged-in account identifier.
    pub user_id: i32,
    /// Session-visible nickname.
    pub display_name: String,
    /// Session-visible icon identifier.
    pub icon_id: u16,
    /// Packed status flags used by Hotline user-list clients.
    pub status_flags: u16,
}

impl PresenceSnapshot {
    /// Encode the SynHX-compatible packed field-300 payload.
    #[must_use]
    pub fn encode_user_name_with_info(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(8 + self.display_name.len());
        payload.extend_from_slice(&truncate_i32_to_u16(self.user_id).to_be_bytes());
        payload.extend_from_slice(&self.icon_id.to_be_bytes());
        payload.extend_from_slice(&self.status_flags.to_be_bytes());
        payload.extend_from_slice(&truncate_len_to_u16(self.display_name.len()).to_be_bytes());
        payload.extend_from_slice(self.display_name.as_bytes());
        payload
    }

    fn notify_change_fields(&self) -> [(FieldId, Vec<u8>); 4] {
        [
            (FieldId::UserId, self.user_id.to_be_bytes().to_vec()),
            (FieldId::IconId, self.icon_id.to_be_bytes().to_vec()),
            (
                FieldId::UserFlags,
                u32::from(self.status_flags).to_be_bytes().to_vec(),
            ),
            (FieldId::Name, self.display_name.as_bytes().to_vec()),
        ]
    }
}

/// Result of removing a connection from the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceRemoval {
    /// Snapshot that was removed, if the connection was online.
    pub departed: PresenceSnapshot,
    /// Remaining online peers after removal.
    pub remaining_peer_ids: Vec<OutboundConnectionId>,
}

/// Shared runtime registry of online presence snapshots.
#[derive(Debug, Default)]
pub struct PresenceRegistry {
    snapshots: Mutex<HashMap<OutboundConnectionId, PresenceSnapshot>>,
}

impl PresenceRegistry {
    /// Insert or replace a connection snapshot and return peer targets.
    #[must_use]
    pub fn upsert(&self, snapshot: PresenceSnapshot) -> Vec<OutboundConnectionId> {
        let mut guard = self.lock_snapshots();
        let connection_id = snapshot.connection_id;
        guard.insert(connection_id, snapshot);
        peer_ids_from_guard(&guard, Some(connection_id))
    }

    /// Remove a connection snapshot if it was online.
    #[must_use]
    pub fn remove(&self, connection_id: OutboundConnectionId) -> Option<PresenceRemoval> {
        let mut guard = self.lock_snapshots();
        let departed = guard.remove(&connection_id)?;
        let remaining_peer_ids = peer_ids_from_guard(&guard, None);
        Some(PresenceRemoval {
            departed,
            remaining_peer_ids,
        })
    }

    /// Return all currently online snapshots in deterministic order.
    #[must_use]
    pub fn online_snapshots(&self) -> Vec<PresenceSnapshot> {
        let guard = self.lock_snapshots();
        sorted_snapshots(guard.values().cloned().collect())
    }

    /// Look up a snapshot by user identifier.
    #[must_use]
    pub fn snapshot_for_user_id(&self, user_id: i32) -> Option<PresenceSnapshot> {
        let guard = self.lock_snapshots();
        guard
            .values()
            .filter(|snapshot| snapshot.user_id == user_id)
            .min_by_key(|snapshot| snapshot.connection_id.as_u64())
            .cloned()
    }

    fn lock_snapshots(&self) -> MutexGuard<'_, HashMap<OutboundConnectionId, PresenceSnapshot>> {
        self.snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Build a `300` reply with repeated field-300 entries.
///
/// # Errors
///
/// Returns an encoding error if the payload would exceed protocol limits.
pub fn build_user_name_list_reply(
    header: &FrameHeader,
    snapshots: &[PresenceSnapshot],
) -> Result<Transaction, TransactionError> {
    let params: Vec<(FieldId, Vec<u8>)> = snapshots
        .iter()
        .map(|snapshot| {
            (
                FieldId::UserNameWithInfo,
                snapshot.encode_user_name_with_info(),
            )
        })
        .collect();
    let payload = encode_params(&params)?;
    Ok(Transaction {
        header: reply_header(header, 0, payload.len()),
        payload,
    })
}

/// Build a `301` notification transaction.
///
/// # Errors
///
/// Returns an encoding error if the payload would exceed protocol limits.
pub fn build_notify_change_user(
    snapshot: &PresenceSnapshot,
) -> Result<Transaction, TransactionError> {
    let payload = encode_params(&snapshot.notify_change_fields())?;
    Ok(server_notification(
        TransactionType::NotifyChangeUser,
        payload,
    ))
}

/// Build a `302` notification transaction.
///
/// # Errors
///
/// Returns an encoding error if the payload would exceed protocol limits.
pub fn build_notify_delete_user(user_id: i32) -> Result<Transaction, TransactionError> {
    let payload = encode_params(&[(FieldId::UserId, user_id.to_be_bytes())])?;
    Ok(server_notification(
        TransactionType::NotifyDeleteUser,
        payload,
    ))
}

/// Build a `303` reply transaction.
///
/// # Errors
///
/// Returns an encoding error if the payload would exceed protocol limits.
pub fn build_client_info_text_reply(
    header: &FrameHeader,
    display_name: &str,
    info_text: &str,
) -> Result<Transaction, TransactionError> {
    let payload = encode_params(&[
        (FieldId::Name, display_name.as_bytes()),
        (FieldId::Data, info_text.as_bytes()),
    ])?;
    Ok(Transaction {
        header: reply_header(header, 0, payload.len()),
        payload,
    })
}

fn server_notification(transaction_type: TransactionType, payload: Vec<u8>) -> Transaction {
    let payload_len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    Transaction {
        header: FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: transaction_type.into(),
            id: 0,
            error: 0,
            total_size: payload_len,
            data_size: payload_len,
        },
        payload,
    }
}

fn sorted_snapshots(mut snapshots: Vec<PresenceSnapshot>) -> Vec<PresenceSnapshot> {
    snapshots.sort_by_key(|snapshot| (snapshot.user_id, snapshot.connection_id.as_u64()));
    snapshots
}

fn peer_ids_from_guard(
    guard: &HashMap<OutboundConnectionId, PresenceSnapshot>,
    excluded_id: Option<OutboundConnectionId>,
) -> Vec<OutboundConnectionId> {
    let mut peer_ids: Vec<_> = guard
        .values()
        .filter_map(|snapshot| {
            if excluded_id == Some(snapshot.connection_id) {
                None
            } else {
                Some(snapshot.connection_id)
            }
        })
        .collect();
    peer_ids.sort_by_key(|connection_id| connection_id.as_u64());
    peer_ids
}

fn truncate_i32_to_u16(value: i32) -> u16 { u16::try_from(value).unwrap_or(u16::MAX) }

fn truncate_len_to_u16(value: usize) -> u16 { u16::try_from(value).unwrap_or(u16::MAX) }

#[cfg(test)]
mod tests {
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

        let encoded = snapshot.encode_user_name_with_info();

        assert_eq!(&encoded[0..2], &7u16.to_be_bytes());
        assert_eq!(&encoded[2..4], &9u16.to_be_bytes());
        assert_eq!(&encoded[4..6], &2u16.to_be_bytes());
        assert_eq!(&encoded[6..8], &5u16.to_be_bytes());
        assert_eq!(&encoded[8..], b"alice");
    }

    #[test]
    fn registry_returns_sorted_snapshots() {
        let registry = PresenceRegistry::default();
        let _ = registry.upsert(snapshot(3, 9, "charlie"));
        let _ = registry.upsert(snapshot(1, 2, "alice"));
        let _ = registry.upsert(snapshot(2, 2, "alice-2"));

        let snapshots = registry.online_snapshots();

        assert_eq!(snapshots[0].connection_id, OutboundConnectionId::new(1));
        assert_eq!(snapshots[1].connection_id, OutboundConnectionId::new(2));
        assert_eq!(snapshots[2].connection_id, OutboundConnectionId::new(3));
    }

    #[test]
    fn registry_remove_returns_remaining_peers() {
        let registry = PresenceRegistry::default();
        let _ = registry.upsert(snapshot(1, 1, "alice"));
        let _ = registry.upsert(snapshot(3, 3, "charlie"));
        let _ = registry.upsert(snapshot(2, 2, "bob"));

        let removed = registry
            .remove(OutboundConnectionId::new(2))
            .unwrap_or_else(|| panic!("removed snapshot"));

        assert_eq!(removed.departed.user_id, 2);
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
        let reply = build_user_name_list_reply(
            &header,
            &[
                {
                    let mut user = snapshot(1, 1, "alice");
                    user.icon_id = 4;
                    user
                },
                {
                    let mut user = snapshot(2, 2, "bob");
                    user.icon_id = 5;
                    user.status_flags = 2;
                    user
                },
            ],
        )
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
}
