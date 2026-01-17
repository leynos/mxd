//! Shared helpers for integration tests.

#[cfg(feature = "postgres")]
use test_util::postgres::PostgresTestDbError;
use test_util::{AnyError, DatabaseUrl, TestServer, ensure_server_binary_env};

/// Start the server for a test or skip if prerequisites are unavailable.
///
/// Runs the provided setup callback, returning a started `TestServer` on success or `None` when the
/// environment indicates the test should be skipped (e.g., embedded Postgres not available).
///
/// # Errors
///
/// Returns any error produced by the setup callback or while launching the server.
pub fn start_server_or_skip<F>(setup: F) -> Result<Option<TestServer>, AnyError>
where
    F: FnOnce(DatabaseUrl) -> Result<(), AnyError>,
{
    ensure_server_binary_env(env!("CARGO_BIN_EXE_mxd-wireframe-server"))?;
    match TestServer::start_with_setup("./Cargo.toml", |db| setup(DatabaseUrl::from(db))) {
        Ok(s) => Ok(Some(s)),
        Err(e) => {
            #[cfg(feature = "postgres")]
            if e.downcast_ref::<PostgresTestDbError>()
                .is_some_and(PostgresTestDbError::is_unavailable)
            {
                tracing::warn!("skipping test: {e}");
                return Ok(None);
            }
            Err(e)
        }
    }
}
