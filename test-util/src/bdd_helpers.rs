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

async fn run_setup_fn(
    setup: SetupFn,
    db_url: DatabaseUrl,
    setup_context: &'static str,
    join_context: &'static str,
) -> Result<(), AnyError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        let (result_tx, result_rx) = tokio::sync::oneshot::channel::<Result<(), AnyError>>();
        std::thread::spawn(move || {
            let _send_result = result_tx.send(setup(db_url));
        });
        result_rx
            .await
            .context(join_context)?
            .context(setup_context)?;
    } else {
        setup(db_url).context(setup_context)?;
    }
    Ok(())
}

/// Build a test database, returning `None` when the backend is unavailable.
///
/// # Errors
///
/// Returns any error raised while creating the database or connection pool.
pub async fn build_test_db_async(setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path().join("mxd.db");
        let db_url_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("database path is not valid UTF-8"))?;
        let db_url = DatabaseUrl::from(db_url_str);
        run_setup_fn(
            setup,
            db_url.clone(),
            "failed to run SQLite test database setup",
            "failed to receive SQLite setup result",
        )
        .await?;
        let pool = establish_pool(db_url.as_str())
            .await
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
        run_setup_fn(
            setup,
            db_url.clone(),
            "failed to run Postgres test database setup",
            "failed to receive Postgres setup result",
        )
        .await?;
        let pool = establish_pool(db_url.as_str())
            .await
            .context("failed to establish Postgres connection pool")?;
        Ok(Some(TestDb {
            pool,
            _postgres: db,
        }))
    }

    #[cfg(not(any(feature = "sqlite", feature = "postgres")))]
    {
        let _ = setup;
        Ok(None)
    }
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
