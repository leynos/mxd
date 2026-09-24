//! Serial behaviour of the presence table.
//!
//! The server's own tests cover the registry built on this table; these pin
//! the allocator's order, which the Loom models' expected outcomes rely on.

use super::*;

/// A minimal entry: a connection key and the presence ID it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    key: u64,
    presence_id: i32,
}

impl PresenceEntry for Entry {
    type Key = u64;

    fn key(&self) -> u64 { self.key }

    fn presence_id(&self) -> i32 { self.presence_id }

    fn assign_presence_id(&mut self, presence_id: i32) { self.presence_id = presence_id; }
}

/// Return an entry for connection `key` with no ID assigned yet.
const fn entry(key: u64) -> Entry {
    Entry {
        key,
        presence_id: 0,
    }
}

/// Upsert `key`, failing the test if the table refuses it.
fn upsert_id(table: &PresenceTable<Entry>, key: u64) -> i32 {
    table
        .upsert(entry(key))
        .unwrap_or_else(|exhausted| panic!("the table has free IDs: {exhausted:?}"))
        .entry
        .presence_id
}

/// IDs are issued from 1 upwards, and a connection that upserts again keeps
/// the ID it already holds.
#[test]
fn issues_ids_in_order_and_keeps_them_across_updates() {
    let table = PresenceTable::default();
    assert_eq!(upsert_id(&table, 10), 1);
    assert_eq!(upsert_id(&table, 20), 2);
    assert_eq!(upsert_id(&table, 10), 1);
}

/// A departed connection's ID is not reissued until the cursor wraps back
/// round to it.
#[test]
fn does_not_reissue_a_departed_id_immediately() {
    let table = PresenceTable::default();
    assert_eq!(upsert_id(&table, 10), 1);
    assert!(table.remove(10).is_some());
    assert_eq!(upsert_id(&table, 20), 2);
}

/// Upsert reports every other active connection, and remove reports every
/// connection left.
#[test]
fn reports_peers_and_remaining_connections() {
    let table = PresenceTable::default();
    upsert_id(&table, 10);
    let mut peers = table.upsert(entry(20)).expect("free IDs").peers;
    peers.sort_unstable();
    assert_eq!(peers, [10]);

    let removed = table.remove(10).expect("connection 10 is active");
    assert_eq!(removed.departed.key, 10);
    assert_eq!(removed.remaining, [20]);
    assert!(table.remove(10).is_none());
}
