//! The lock the kernels share their state behind.
//!
//! An ordinary build uses the standard library's mutex. Under `--cfg loom` it
//! is Loom's, which has the same signature, so Loom schedules every
//! acquisition. Both return a `LockResult`, and the kernels recover the guard
//! from a poisoned lock rather than propagating the poison, as the server did
//! before the kernels were extracted.

#[cfg(not(loom))]
pub(crate) use std::sync::{Mutex, MutexGuard};

#[cfg(loom)]
pub(crate) use loom::sync::{Mutex, MutexGuard};

/// Acquire `mutex`, recovering the guard if a panicking holder poisoned it.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
