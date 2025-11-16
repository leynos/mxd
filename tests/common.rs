use std::sync::Once;

#[cfg(feature = "postgres")]
use test_util::postgres::PostgresUnavailable;
use test_util::{AnyError, TestServer};

fn ensure_server_binary_env() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if std::env::var_os("CARGO_BIN_EXE_mxd").is_none() {
            if let Some(path) = option_env!("CARGO_BIN_EXE_mxd") {
                // SAFETY: tests run single-threaded when the env var is set, so mutating the
                // process environment is acceptable here.
                unsafe { std::env::set_var("CARGO_BIN_EXE_mxd", path) };
            }
        }
    });
}

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
    ensure_server_binary_env();
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
