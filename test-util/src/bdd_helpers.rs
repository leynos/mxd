//! Helpers for wireframe routing BDD integration tests.

// The lint feature enables combined sqlite/postgres builds for full static
// analysis coverage, so only enforce exclusivity outside lint runs.
#[cfg(all(feature = "sqlite", feature = "postgres", not(feature = "lint")))]
compile_error!("Choose either sqlite or postgres, not both");

use anyhow::Context as _;
use mxd::db::{DbPool, establish_pool};
#[cfg(feature = "sqlite")]
use tempfile::TempDir;
use tokio::runtime::Runtime;

#[cfg(feature = "postgres")]
use crate::postgres::PostgresTestDb;
use crate::{AnyError, DatabaseUrl};

/// Fixture database setup function signature.
pub type SetupFn = fn(DatabaseUrl) -> Result<(), AnyError>;

/// Holds a database pool and the guard needed to keep the database alive.
pub struct TestDb {
    pool: DbPool,
    #[cfg(feature = "sqlite")]
    _temp_dir: TempDir,
    #[cfg(feature = "postgres")]
    _postgres: PostgresTestDb,
}

impl TestDb {
    /// Clone the underlying database pool.
    #[must_use]
    pub fn pool(&self) -> DbPool { self.pool.clone() }
}

/// Build a test database, returning `None` when the backend is unavailable.
///
/// # Errors
///
/// Returns any error raised while creating the database or connection pool.
pub fn build_test_db(rt: &Runtime, setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path().join("mxd.db");
        let db_url_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("database path is not valid UTF-8"))?;
        let db_url = DatabaseUrl::from(db_url_str);
        setup(db_url.clone()).context("failed to run SQLite test database setup")?;
        let pool = rt
            .block_on(establish_pool(db_url.as_str()))
            .context("failed to establish SQLite connection pool")?;
        Ok(Some(TestDb {
            pool,
            _temp_dir: temp_dir,
        }))
    }

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    {
        let db = match PostgresTestDb::new() {
            Ok(db) => db,
            Err(err) if err.is_unavailable() => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let db_url = DatabaseUrl::from(db.url.as_ref());
        setup(db_url.clone()).context("failed to run Postgres test database setup")?;
        let pool = rt
            .block_on(establish_pool(db_url.as_str()))
            .context("failed to establish Postgres connection pool")?;
        Ok(Some(TestDb {
            pool,
            _postgres: db,
        }))
    }

    #[cfg(not(any(feature = "sqlite", feature = "postgres")))]
    {
        let _ = (rt, setup);
        Ok(None)
    }
}
