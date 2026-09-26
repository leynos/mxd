//! A registry of per-task connection contexts.
//!
//! The Wireframe adapter mirrors each connection's context into a map keyed by
//! the Tokio task identifier, so synchronous app-factory code running later in
//! the same task can still find the context the handshake stored. Many
//! connection tasks store, read and take their own entries concurrently; the
//! registry's contract is that each task only ever sees and removes its own.

use std::{collections::HashMap, hash::Hash};

use crate::sync::{Mutex, lock};

/// Connection contexts keyed by the task that owns them.
#[derive(Debug)]
pub struct ContextRegistry<Id, C> {
    entries: Mutex<HashMap<Id, C>>,
}

impl<Id, C> Default for ContextRegistry<Id, C> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<Id: Eq + Hash, C: Clone> ContextRegistry<Id, C> {
    /// Store `context` for task `id`, replacing any context it held.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd_concurrency::context::ContextRegistry;
    ///
    /// let registry = ContextRegistry::default();
    /// registry.store(1, "chat");
    /// assert_eq!(registry.get(&1), Some("chat"));
    /// ```
    pub fn store(&self, id: Id, context: C) { lock(&self.entries).insert(id, context); }

    /// Return a copy of the context stored for task `id`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd_concurrency::context::ContextRegistry;
    ///
    /// let registry: ContextRegistry<u64, &str> = ContextRegistry::default();
    /// assert_eq!(registry.get(&1), None);
    /// ```
    #[must_use]
    pub fn get(&self, id: &Id) -> Option<C> { lock(&self.entries).get(id).cloned() }

    /// Remove and return the context stored for task `id`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd_concurrency::context::ContextRegistry;
    ///
    /// let registry = ContextRegistry::default();
    /// registry.store(1, "chat");
    /// assert_eq!(registry.take(&1), Some("chat"));
    /// assert_eq!(registry.get(&1), None);
    /// ```
    #[must_use]
    pub fn take(&self, id: &Id) -> Option<C> { lock(&self.entries).remove(id) }

    /// Count the stored contexts accepted by `predicate`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd_concurrency::context::ContextRegistry;
    ///
    /// let registry = ContextRegistry::default();
    /// registry.store(1, "chat");
    /// registry.store(2, "files");
    /// assert_eq!(registry.count(|context| *context == "chat"), 1);
    /// ```
    #[must_use]
    pub fn count(&self, predicate: impl Fn(&C) -> bool) -> usize {
        lock(&self.entries)
            .values()
            .filter(|context| predicate(context))
            .count()
    }
}

#[cfg(all(test, not(loom)))]
#[path = "context_tests.rs"]
mod tests;
