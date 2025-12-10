//! Utilities for integration tests.
//!
//! The `test-util` crate provides helpers to spin up temporary servers and,
//! when the `postgres` feature is enabled, manage embedded `PostgreSQL`
//! instances. It is used by integration tests in the main crate.

#![expect(missing_docs, reason = "test utility crate; API docs not required")]
#![expect(
    clippy::missing_errors_doc,
    reason = "test utility crate; error docs not required"
)]

/// A boxed error type for test functions.
pub type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[cfg(feature = "postgres")]
pub mod postgres;

mod fixtures;
mod protocol;
mod server;

pub use fixtures::{
    setup_files_db,
    setup_news_categories_nested_db,
    setup_news_categories_root_db,
    setup_news_categories_with_structure,
    setup_news_db,
    with_db,
};
#[cfg(feature = "postgres")]
pub use postgres::{PostgresTestDb, postgres_db};
pub use protocol::handshake;
pub use server::{TestServer, ensure_server_binary_env};
