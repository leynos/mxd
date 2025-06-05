//! Diesel extension crate providing support for recursive CTEs.
//!
//! The [`with_recursive`] function builds a Diesel query representing a
//! `WITH RECURSIVE` block that can be executed like any other query.

pub mod builders;
pub mod cte;

pub use builders::with_recursive;
pub use cte::RecursiveBackend;
