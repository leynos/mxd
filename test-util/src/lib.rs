//! Utilities for integration tests.
//!
//! The `test-util` crate provides helpers to spin up temporary servers and,
//! when the `postgres` feature is enabled, manage embedded `PostgreSQL`
//! instances. It is used by integration tests in the main crate.

/// A boxed error type for test functions.
pub type AnyError = anyhow::Error;

#[cfg(feature = "postgres")]
pub mod postgres;

mod bdd_helpers;
mod fixtures;
mod protocol;
mod server;

pub use bdd_helpers::{SetupFn, TestDb, build_test_db};
pub use fixtures::{
    DatabaseUrl,
    ensure_test_user,
    setup_files_db,
    setup_news_categories_nested_db,
    setup_news_categories_root_db,
    setup_news_categories_with_structure,
    setup_news_db,
    setup_news_with_article,
    with_db,
};
pub use mxd::wireframe::test_helpers::{build_frame, collect_strings};
#[cfg(feature = "postgres")]
pub use postgres::{PostgresTestDb, postgres_db};
pub use protocol::{handshake, login};
pub use server::{TestServer, ensure_server_binary_env, with_env_var};
