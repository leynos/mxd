#![allow(missing_docs, reason = "test helpers")]
#![allow(clippy::print_stderr, reason = "test diagnostics")]

#[cfg(feature = "postgres")]
use test_util::postgres::PostgresUnavailable;
use test_util::{AnyError, TestServer, ensure_server_binary_env};

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
    F: FnOnce(&str) -> Result<(), AnyError>,
{
    ensure_server_binary_env(env!("CARGO_BIN_EXE_mxd"))?;
    match TestServer::start_with_setup("./Cargo.toml", |db| setup(db.as_str())) {
        Ok(s) => Ok(Some(s)),
        Err(e) => {
            #[cfg(feature = "postgres")]
            if e.downcast_ref::<PostgresUnavailable>().is_some() {
                eprintln!("skipping test: {e}");
                return Ok(None);
            }
            Err(e)
        }
    }
}
