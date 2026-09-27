//! Serial behaviour of the presence table.
//!
//! The server's own tests cover the registry built on this table; these pin
//! the allocator's order, which the Loom models' expected outcomes rely on.

use rstest::rstest;

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

/// One step of an allocation scenario, with the outcome it must produce.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// Connection `key` upserts and must hold presence ID `id`.
    Arrive(u64, i32),
    /// Connection `key` is removed and must have been active.
    Depart(u64),
    /// Connection `key` upserts and must be refused: every ID is held.
    Refused(u64),
}

/// Play `steps` against a table issuing IDs up to `ceiling`.
fn play(ceiling: u16, steps: &[Step]) {
    let table = PresenceTable::with_ceiling(ceiling);
    for (index, step) in steps.iter().enumerate() {
        match *step {
            Step::Arrive(key, id) => {
                let outcome = table
                    .upsert(entry(key))
                    .map(|upserted| upserted.entry.presence_id);
                assert_eq!(outcome, Ok(id), "step {index}: {step:?}");
            }
            Step::Depart(key) => {
                assert!(table.remove(key).is_some(), "step {index}: {step:?}");
            }
            Step::Refused(key) => {
                assert_eq!(
                    table.upsert(entry(key)).map(|_| ()),
                    Err(PresenceIdsExhausted),
                    "step {index}: {step:?}"
                );
            }
        }
    }
}

/// Allocation scenarios, each a sequence of arrivals and departures:
///
/// - IDs are issued from 1 upwards, and a connection that upserts again keeps the ID it already
///   holds;
/// - a departed connection's ID is not reissued until the cursor wraps back round to it;
/// - once every ID up to the ceiling is held an arrival is refused, and a departure frees its ID
///   for the next arrival;
/// - the cursor wraps from the ceiling back to 1, never issuing 0.
#[rstest]
#[case::in_order_and_kept(u16::MAX, &[Step::Arrive(10, 1), Step::Arrive(20, 2), Step::Arrive(10, 1)])]
#[case::departed_id_not_reissued(u16::MAX, &[Step::Arrive(10, 1), Step::Depart(10), Step::Arrive(20, 2)])]
#[case::refused_when_exhausted(
    2,
    &[Step::Arrive(10, 1), Step::Arrive(20, 2), Step::Refused(30), Step::Depart(10), Step::Arrive(30, 1)]
)]
#[case::wraps_to_one(
    2,
    &[Step::Arrive(10, 1), Step::Depart(10), Step::Arrive(20, 2), Step::Depart(20), Step::Arrive(30, 1)]
)]
fn allocates_presence_ids(#[case] ceiling: u16, #[case] steps: &[Step]) { play(ceiling, steps); }

/// Upsert reports every other active connection, and remove reports every
/// connection left.
#[test]
fn reports_peers_and_remaining_connections() {
    let table = PresenceTable::default();
    table.upsert(entry(10)).expect("free IDs");
    let mut peers = table.upsert(entry(20)).expect("free IDs").peers;
    peers.sort_unstable();
    assert_eq!(peers, [10]);

    let removed = table.remove(10).expect("connection 10 is active");
    assert_eq!(removed.departed.key, 10);
    assert_eq!(removed.remaining, [20]);
    assert!(table.remove(10).is_none());
}
