//! Shared helpers for integration tests.

use std::path::Path;

use test_util::{AnyError, DatabaseUrl, TestServer};

/// Start the server for a test after running a database setup callback.
///
/// # Errors
///
/// Returns any error produced by the setup callback or while launching the
/// server, including an embedded `PostgreSQL` cluster that fails to start:
/// that is a failure, never a reason to skip.
pub fn start_server<F>(setup: F) -> Result<TestServer, AnyError>
where
    F: FnOnce(DatabaseUrl) -> Result<(), AnyError>,
{
    TestServer::start_with_binary_and_setup(
        "./Cargo.toml",
        Some(Path::new(env!("CARGO_BIN_EXE_mxd-wireframe-server"))),
        |db| setup(DatabaseUrl::from(db)),
    )
}
