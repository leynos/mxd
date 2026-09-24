//! Serial behaviour of the context registry.

use super::*;

/// Each task sees and removes only its own context.
#[test]
fn keeps_contexts_apart_by_task() {
    let registry = ContextRegistry::default();
    registry.store(1, "chat");
    registry.store(2, "files");

    assert_eq!(registry.take(&1), Some("chat"));
    assert_eq!(registry.get(&1), None);
    assert_eq!(registry.get(&2), Some("files"));
    assert_eq!(registry.count(|_| true), 1);
}

/// Storing again for the same task replaces its context.
#[test]
fn replaces_a_task_s_context() {
    let registry = ContextRegistry::default();
    registry.store(1, "chat");
    registry.store(1, "files");
    assert_eq!(registry.get(&1), Some("files"));
    assert_eq!(registry.count(|_| true), 1);
}
