//! Shared helpers for integration tests.

use test_util::{AnyError, DatabaseUrl, TestServer, ensure_server_binary_env};

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
    ensure_server_binary_env(env!("CARGO_BIN_EXE_mxd-wireframe-server"))?;
    TestServer::start_with_setup("./Cargo.toml", |db| setup(DatabaseUrl::from(db)))
}
