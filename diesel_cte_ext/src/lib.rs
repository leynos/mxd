//! Diesel extension crate providing support for recursive CTEs.
//!
//! The [`RecursiveCTEExt::with_recursive`] method builds a Diesel query
//! representing a `WITH RECURSIVE` block that can be executed like any other
//! query.

pub mod builders;
pub mod connection_ext;
pub mod cte;

pub use builders::{RecursiveParts, with_recursive};
pub use connection_ext::RecursiveCTEExt;
pub use cte::RecursiveBackend;
