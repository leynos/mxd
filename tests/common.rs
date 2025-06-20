use std::error::Error;

use test_util::TestServer;
#[cfg(feature = "postgres")]
use test_util::postgres::PostgresUnavailable;

pub fn start_server_or_skip<F>(setup: F) -> Result<Option<TestServer>, Box<dyn Error>>
where
    F: FnOnce(&str) -> Result<(), Box<dyn Error>>,
{
    match TestServer::start_with_setup("./Cargo.toml", setup) {
        Ok(s) => Ok(Some(s)),
        Err(e) => {
            #[cfg(feature = "postgres")]
            if e.downcast_ref::<PostgresUnavailable>().is_some() {
                eprintln!("skipping test: {}", e);
                return Ok(None);
            }
            Err(e)
        }
    }
}
