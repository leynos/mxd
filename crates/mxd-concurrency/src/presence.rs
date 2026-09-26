//! The online-presence table and its presence-ID allocator.
//!
//! Every connection that comes online is upserted here and receives a
//! presence ID, the protocol-visible user ID other clients see in their user
//! lists. Connections come and go concurrently, so the table's contract is
//! about interleavings: two active connections never hold the same ID, a
//! connection keeps its ID across updates, and every result is one some
//! serial order of the same operations would have produced.
//!
//! IDs are allocated round-robin from a cursor, skipping IDs still held, so a
//! departed connection's ID is not reissued until the cursor comes back round.

use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

use crate::sync::{Mutex, MutexGuard, lock};

/// A presence record the table can key, and assign an ID to.
pub trait PresenceEntry: Clone + Debug {
    /// The connection the entry belongs to.
    type Key: Copy + Eq + Hash + Debug;

    /// Return the connection the entry belongs to.
    fn key(&self) -> Self::Key;

    /// Return the entry's presence ID.
    fn presence_id(&self) -> i32;

    /// Replace the entry's presence ID with the one the table assigned.
    fn assign_presence_id(&mut self, presence_id: i32);
}

/// Every presence ID from 1 to `u16::MAX` is held by an active entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresenceIdsExhausted;

/// The outcome of an upsert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upserted<E: PresenceEntry> {
    /// The entry as stored, carrying its assigned presence ID.
    pub entry: E,
    /// Every other active connection, in no particular order.
    pub peers: Vec<E::Key>,
}

/// The outcome of removing an active connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removed<E: PresenceEntry> {
    /// The entry that was removed.
    pub departed: E,
    /// Every connection still active, in no particular order.
    pub remaining: Vec<E::Key>,
}

/// The table's state, guarded as one unit.
#[derive(Debug)]
struct State<E: PresenceEntry> {
    entries: HashMap<E::Key, E>,
    cursor: u16,
}

/// Active presence entries, keyed by connection.
#[derive(Debug)]
pub struct PresenceTable<E: PresenceEntry> {
    state: Mutex<State<E>>,
}

impl<E: PresenceEntry> Default for PresenceTable<E> {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                entries: HashMap::new(),
                cursor: 0,
            }),
        }
    }
}

impl<E: PresenceEntry> PresenceTable<E> {
    /// Insert or replace `entry`, assigning it a presence ID.
    ///
    /// A connection already present keeps the ID it holds. The ID is chosen
    /// and the entry stored under one acquisition of the lock, which is what
    /// keeps two concurrent upserts from choosing the same free ID.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceIdsExhausted`] when every ID is held.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd_concurrency::presence::{PresenceEntry, PresenceTable};
    ///
    /// #[derive(Clone, Debug)]
    /// struct Entry(u64, i32);
    ///
    /// impl PresenceEntry for Entry {
    ///     type Key = u64;
    ///     fn key(&self) -> u64 { self.0 }
    ///     fn presence_id(&self) -> i32 { self.1 }
    ///     fn assign_presence_id(&mut self, presence_id: i32) { self.1 = presence_id; }
    /// }
    ///
    /// let table = PresenceTable::default();
    /// let first = table.upsert(Entry(7, 0)).expect("IDs are free");
    /// assert_eq!(first.entry.presence_id(), 1);
    /// assert!(first.peers.is_empty());
    /// ```
    pub fn upsert(&self, mut entry: E) -> Result<Upserted<E>, PresenceIdsExhausted> {
        let mut state = self.lock_state();
        let key = entry.key();
        entry.assign_presence_id(state.assigned_presence_id(key)?);
        state.entries.insert(key, entry.clone());
        let peers = state.keys_except(Some(key));
        Ok(Upserted { entry, peers })
    }

    /// Remove the entry for connection `key`, if it is active.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd_concurrency::presence::{PresenceEntry, PresenceTable};
    ///
    /// #[derive(Clone, Debug)]
    /// struct Entry(u64, i32);
    ///
    /// impl PresenceEntry for Entry {
    ///     type Key = u64;
    ///     fn key(&self) -> u64 { self.0 }
    ///     fn presence_id(&self) -> i32 { self.1 }
    ///     fn assign_presence_id(&mut self, presence_id: i32) { self.1 = presence_id; }
    /// }
    ///
    /// let table = PresenceTable::default();
    /// assert!(table.remove(7).is_none());
    /// table.upsert(Entry(7, 0)).expect("IDs are free");
    /// assert_eq!(table.remove(7).map(|removed| removed.departed.0), Some(7));
    /// ```
    #[must_use]
    pub fn remove(&self, key: E::Key) -> Option<Removed<E>> {
        let mut state = self.lock_state();
        let departed = state.entries.remove(&key)?;
        let remaining = state.keys_except(None);
        Some(Removed {
            departed,
            remaining,
        })
    }

    /// Run `read` over the active entries while holding the lock.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd_concurrency::presence::{PresenceEntry, PresenceTable};
    ///
    /// #[derive(Clone, Debug)]
    /// struct Entry(u64, i32);
    ///
    /// impl PresenceEntry for Entry {
    ///     type Key = u64;
    ///     fn key(&self) -> u64 { self.0 }
    ///     fn presence_id(&self) -> i32 { self.1 }
    ///     fn assign_presence_id(&mut self, presence_id: i32) { self.1 = presence_id; }
    /// }
    ///
    /// let table = PresenceTable::default();
    /// table.upsert(Entry(7, 0)).expect("IDs are free");
    /// assert_eq!(table.read(|entries| entries.len()), 1);
    /// ```
    pub fn read<R>(&self, read: impl FnOnce(&HashMap<E::Key, E>) -> R) -> R {
        read(&self.lock_state().entries)
    }

    /// Acquire the table's state.
    fn lock_state(&self) -> MutexGuard<'_, State<E>> { lock(&self.state) }
}

impl<E: PresenceEntry> State<E> {
    /// Return the ID connection `key` holds, or allocate a free one.
    fn assigned_presence_id(&mut self, key: E::Key) -> Result<i32, PresenceIdsExhausted> {
        if let Some(entry) = self.entries.get(&key) {
            return Ok(entry.presence_id());
        }
        self.next_free_presence_id()
            .map(i32::from)
            .ok_or(PresenceIdsExhausted)
    }

    /// Advance the cursor to the next ID no active entry holds.
    ///
    /// Zero is never issued. An entry whose ID does not fit in `u16` holds no
    /// allocatable ID.
    fn next_free_presence_id(&mut self) -> Option<u16> {
        let active: HashSet<u16> = self
            .entries
            .values()
            .filter_map(|entry| u16::try_from(entry.presence_id()).ok())
            .collect();
        for _ in 0..u16::MAX {
            self.cursor = self.cursor.wrapping_add(1).max(1);
            if !active.contains(&self.cursor) {
                return Some(self.cursor);
            }
        }
        None
    }

    /// Return every active connection except `excluded`.
    fn keys_except(&self, excluded: Option<E::Key>) -> Vec<E::Key> {
        self.entries
            .keys()
            .copied()
            .filter(|key| Some(*key) != excluded)
            .collect()
    }
}

#[cfg(all(test, not(loom)))]
#[path = "presence_tests.rs"]
mod tests;
