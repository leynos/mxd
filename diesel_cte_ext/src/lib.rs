//! Diesel extension crate providing support for recursive CTEs.
//!
//! The [`RecursiveCTEExt::with_recursive`] method builds a Diesel query
//! representing a `WITH RECURSIVE` block that can be executed like any other
//! query.

pub mod builders;
pub mod columns;
pub mod connection_ext;
pub mod cte;

#[deprecated(note = "Use `RecursiveCTEExt::with_recursive` instead")]
pub use builders::with_recursive;
pub use builders::{RecursiveParts, with_cte};
pub use columns::Columns;
pub use connection_ext::RecursiveCTEExt;
pub use cte::RecursiveBackend;
