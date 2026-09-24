//! Loom models of the connection-context registry.
//!
//! Two connection tasks store, read and take their own contexts while a
//! third connection's context stays registered. Each model builds its own
//! registry, so no state survives from one explored interleaving to the next;
//! in the server the registry is a process-wide `OnceLock`, which is the
//! environment around this kernel rather than part of it.
#![cfg(loom)]

use loom::{sync::Arc, thread};
use mxd_concurrency::context::ContextRegistry;

/// Run one connection task's lifecycle for `id`, returning what it observed.
fn connection_lifecycle(
    registry: &ContextRegistry<u64, &'static str>,
    id: u64,
    context: &'static str,
) -> (
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
) {
    registry.store(id, context);
    let read = registry.get(&id);
    let taken = registry.take(&id);
    (read, taken, registry.get(&id))
}

/// Concurrent connections each see and remove only their own context, and a
/// live connection's context outlasts both.
#[test]
fn loom_connections_see_only_their_own_context() {
    loom::model(|| {
        let registry = Arc::new(ContextRegistry::default());
        registry.store(3, "live");
        let other = {
            let shared = Arc::clone(&registry);
            thread::spawn(move || connection_lifecycle(&shared, 2, "files"))
        };
        let mine = connection_lifecycle(&registry, 1, "chat");
        let theirs = other.join().expect("the connection thread does not panic");

        assert_eq!(mine, (Some("chat"), Some("chat"), None));
        assert_eq!(theirs, (Some("files"), Some("files"), None));
        assert_eq!(registry.get(&3), Some("live"));
        assert_eq!(registry.count(|_| true), 1);
    });
}
