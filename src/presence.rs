//! Runtime presence state and payload helpers.
//!
//! This module keeps the server's online-user view in one place so handlers can
//! build consistent `300`, `301`, `302`, and `303` transactions without
//! re-encoding presence state in multiple branches.

use std::collections::HashMap;

use mxd_concurrency::presence::{PresenceEntry, PresenceTable};

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
    /// Protocol-visible identifier assigned to this active presence session.
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
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError::InvalidParamValue`] if the user ID or
    /// display name length cannot be represented in the field-300 wire format.
    #[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
    pub fn encode_user_name_with_info(&self) -> Result<Vec<u8>, TransactionError> {
        let mut payload = Vec::with_capacity(8 + self.display_name.len());
        payload.extend_from_slice(&field_300_user_id(self.user_id)?.to_be_bytes());
        payload.extend_from_slice(&self.icon_id.to_be_bytes());
        payload.extend_from_slice(&self.status_flags.to_be_bytes());
        payload.extend_from_slice(&field_300_name_len(self.display_name.len())?.to_be_bytes());
        payload.extend_from_slice(self.display_name.as_bytes());
        Ok(payload)
    }

    #[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
    fn notify_change_fields(&self) -> [(FieldId, Vec<u8>); 4] {
        [
            (FieldId::UserId, self.user_id.to_be_bytes().to_vec()),
            (FieldId::IconId, self.icon_id.to_be_bytes().to_vec()),
            (FieldId::UserFlags, self.status_flags.to_be_bytes().to_vec()),
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

/// Result of inserting or updating a presence snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceUpsert {
    /// Snapshot stored in the registry, including its session-unique presence ID.
    pub snapshot: PresenceSnapshot,
    /// Other online peers that should receive update notifications.
    pub peer_ids: Vec<OutboundConnectionId>,
}

impl PresenceEntry for PresenceSnapshot {
    type Key = OutboundConnectionId;

    fn key(&self) -> OutboundConnectionId { self.connection_id }

    fn presence_id(&self) -> i32 { self.user_id }

    fn assign_presence_id(&mut self, presence_id: i32) { self.user_id = presence_id; }
}

/// Shared runtime registry of online presence snapshots.
///
/// The table, its lock and the presence-ID allocator are
/// `mxd_concurrency::presence::PresenceTable`, which the Loom models in that
/// crate check under concurrent upserts and removals. This type adds the
/// protocol's ordering and error mapping.
#[derive(Debug, Default)]
pub struct PresenceRegistry {
    table: PresenceTable<PresenceSnapshot>,
}

impl PresenceRegistry {
    /// Insert or replace a connection snapshot and return peer targets.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError::InvalidParamValue`] when all field-300 user
    /// ID values are already assigned to active sessions.
    pub fn upsert(&self, snapshot: PresenceSnapshot) -> Result<PresenceUpsert, TransactionError> {
        let upserted = self
            .table
            .upsert(snapshot)
            .map_err(|_| invalid_field_300())?;
        Ok(PresenceUpsert {
            snapshot: upserted.entry,
            peer_ids: sorted_peer_ids(upserted.peers),
        })
    }

    /// Remove a connection snapshot if it was online.
    #[must_use]
    pub fn remove(&self, connection_id: OutboundConnectionId) -> Option<PresenceRemoval> {
        let removed = self.table.remove(connection_id)?;
        Some(PresenceRemoval {
            departed: removed.departed,
            remaining_peer_ids: sorted_peer_ids(removed.remaining),
        })
    }

    /// Return all currently online snapshots in deterministic order.
    #[must_use]
    pub fn online_snapshots(&self) -> Vec<PresenceSnapshot> {
        sorted_snapshots(
            self.table
                .read(|snapshots| snapshots.values().cloned().collect()),
        )
    }

    /// Look up the presence snapshot for the given user identifier.
    ///
    /// If multiple connections are registered under the same `user_id` (for
    /// example, duplicate logins), the snapshot with the numerically lowest
    /// `connection_id` is returned. Returns `None` if the user is not currently
    /// online.
    #[must_use]
    pub fn snapshot_for_user_id(&self, user_id: i32) -> Option<PresenceSnapshot> {
        self.table
            .read(|snapshots| snapshot_for_user_id(snapshots, user_id))
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
            snapshot
                .encode_user_name_with_info()
                .map(|payload| (FieldId::UserNameWithInfo, payload))
        })
        .collect::<Result<_, _>>()?;
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
    #[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
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

fn sorted_peer_ids(mut peer_ids: Vec<OutboundConnectionId>) -> Vec<OutboundConnectionId> {
    peer_ids.sort_by_key(|connection_id| connection_id.as_u64());
    peer_ids
}

fn snapshot_for_user_id(
    snapshots: &HashMap<OutboundConnectionId, PresenceSnapshot>,
    user_id: i32,
) -> Option<PresenceSnapshot> {
    snapshots
        .values()
        .filter(|snapshot| snapshot.user_id == user_id)
        .min_by_key(|snapshot| snapshot.connection_id.as_u64())
        .cloned()
}

const fn invalid_field_300() -> TransactionError {
    TransactionError::InvalidParamValue(FieldId::UserNameWithInfo)
}

fn field_300_user_id(value: i32) -> Result<u16, TransactionError> {
    u16::try_from(value).map_err(|_| invalid_field_300())
}

fn field_300_name_len(value: usize) -> Result<u16, TransactionError> {
    u16::try_from(value).map_err(|_| invalid_field_300())
}

#[cfg(test)]
#[path = "presence_tests.rs"]
mod tests;
