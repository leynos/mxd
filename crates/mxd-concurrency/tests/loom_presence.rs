//! Loom models of the presence table.
//!
//! Each model runs the table the server runs, from two threads, and asserts
//! that every explored interleaving produces a result some serial order of
//! the same operations would have produced. Two threads is the whole
//! participant count; the bound is `LOOM_MAX_PREEMPTIONS` as the lane sets it.
#![cfg(loom)]

use loom::{sync::Arc, thread};
use mxd_concurrency::presence::{PresenceEntry, PresenceTable, Upserted};

/// A connection key and the presence ID the table assigned it.
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

/// Upsert connection `key` with no ID of its own yet.
fn upsert(table: &PresenceTable<Entry>, key: u64) -> Upserted<Entry> {
    table
        .upsert(Entry {
            key,
            presence_id: 0,
        })
        .unwrap_or_else(|exhausted| panic!("a two-connection table has free IDs: {exhausted:?}"))
}

/// Return every active connection with its ID, ordered by connection.
fn active(table: &PresenceTable<Entry>) -> Vec<(u64, i32)> {
    let mut active = table.read(|entries| {
        entries
            .values()
            .map(|entry| (entry.key, entry.presence_id))
            .collect::<Vec<_>>()
    });
    active.sort_unstable();
    active
}

/// Two connections coming online together get different IDs, and the pair
/// of results is one a serial order produces: the connection ordered first
/// sees no peers and gets ID 1, and the one ordered second sees it and gets
/// ID 2.
#[test]
fn loom_concurrent_upserts_get_distinct_ids() {
    loom::model(|| {
        let table = Arc::new(PresenceTable::default());
        let other = {
            let shared = Arc::clone(&table);
            thread::spawn(move || upsert(&shared, 2))
        };
        let first = upsert(&table, 1);
        let second = other.join().expect("the upserting thread does not panic");

        let outcome = (
            (first.entry.presence_id, first.peers),
            (second.entry.presence_id, second.peers),
        );
        assert!(
            outcome == ((1, vec![]), (2, vec![1])) || outcome == ((2, vec![2]), (1, vec![])),
            "upserts match no serial order: {outcome:?}"
        );
        assert_eq!(active(&table).len(), 2);
    });
}

/// A departure racing an arrival leaves a result one of the two serial
/// orders produces: the arrival either sees the departing connection or it
/// does not, and the departure's remaining list agrees.
#[test]
fn loom_departure_racing_arrival_is_serializable() {
    loom::model(|| {
        let table = Arc::new(PresenceTable::default());
        upsert(&table, 1);
        let arrival = {
            let shared = Arc::clone(&table);
            thread::spawn(move || upsert(&shared, 2))
        };
        let departure = table.remove(1).expect("connection 1 is active");
        let arrived = arrival.join().expect("the upserting thread does not panic");

        let outcome = (departure.remaining, arrived.peers);
        assert!(
            outcome == (vec![], vec![]) || outcome == (vec![2], vec![1]),
            "departure and arrival match no serial order: {outcome:?}"
        );
        assert_eq!(arrived.entry.presence_id, 2);
        assert_eq!(active(&table), [(2, 2)]);
    });
}

/// A connection updating its presence keeps its ID while another connection
/// comes online beside it.
#[test]
fn loom_update_keeps_its_id_beside_an_arrival() {
    loom::model(|| {
        let table = Arc::new(PresenceTable::default());
        upsert(&table, 1);
        let arrival = {
            let shared = Arc::clone(&table);
            thread::spawn(move || upsert(&shared, 2))
        };
        let updated = upsert(&table, 1);
        let arrived = arrival.join().expect("the upserting thread does not panic");

        assert_eq!(updated.entry.presence_id, 1);
        assert_eq!(arrived.entry.presence_id, 2);
        assert_eq!(active(&table), [(1, 1), (2, 2)]);
    });
}

/// A reader never sees an entry without its assigned ID, and sees arrivals
/// in the order they happened.
#[test]
fn loom_reader_sees_only_whole_upserts() {
    loom::model(|| {
        let table = Arc::new(PresenceTable::default());
        let writer = {
            let shared = Arc::clone(&table);
            thread::spawn(move || {
                upsert(&shared, 1);
                upsert(&shared, 2);
            })
        };
        let seen = active(&table);
        writer.join().expect("the upserting thread does not panic");

        assert!(
            [vec![], vec![(1, 1)], vec![(1, 1), (2, 2)]].contains(&seen),
            "reader saw a partial upsert: {seen:?}"
        );
    });
}
