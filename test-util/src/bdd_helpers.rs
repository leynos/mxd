//! Helpers for wireframe routing BDD integration tests.

use std::any::Any;

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

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn sqlite_temp_dir_and_url() -> Result<(TempDir, DatabaseUrl), AnyError> {
    let temp_dir = TempDir::new()?;
    let path = temp_dir.path().join("mxd.db");
    let db_url_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("database path is not valid UTF-8"))?;
    let db_url = DatabaseUrl::from(db_url_str);
    Ok((temp_dir, db_url))
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
const fn sqlite_test_db(pool: DbPool, temp_dir: TempDir) -> TestDb {
    TestDb {
        pool,
        _temp_dir: temp_dir,
    }
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
fn postgres_fixture_and_url() -> Result<Option<(PostgresTestDb, DatabaseUrl)>, AnyError> {
    let db = match PostgresTestDb::new() {
        Ok(db) => db,
        Err(err) if err.is_unavailable() => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let db_url = DatabaseUrl::from(db.url.as_ref());
    Ok(Some((db, db_url)))
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
const fn postgres_test_db(pool: DbPool, db: PostgresTestDb) -> TestDb {
    TestDb {
        pool,
        _postgres: db,
    }
}

fn panic_payload_to_string(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    "non-string panic payload".to_owned()
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
            let setup_result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| setup(db_url)))
                    .map_err(|payload| {
                        anyhow::anyhow!(
                            "setup function panicked: {}",
                            panic_payload_to_string(payload.as_ref())
                        )
                    })
                    .and_then(|result| result);
            let _send_result = result_tx.send(setup_result);
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

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn build_sqlite_test_db_async(setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    let (temp_dir, db_url) = sqlite_temp_dir_and_url()?;
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
    Ok(Some(sqlite_test_db(pool, temp_dir)))
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn build_sqlite_test_db(rt: &Runtime, setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    let (temp_dir, db_url) = sqlite_temp_dir_and_url()?;
    setup(db_url.clone()).context("failed to run SQLite test database setup")?;
    let pool = rt
        .block_on(establish_pool(db_url.as_str()))
        .context("failed to establish SQLite connection pool")?;
    Ok(Some(sqlite_test_db(pool, temp_dir)))
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
async fn build_postgres_test_db_async(setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    let Some((db, db_url)) = postgres_fixture_and_url()? else {
        return Ok(None);
    };
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
    Ok(Some(postgres_test_db(pool, db)))
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
fn build_postgres_test_db(rt: &Runtime, setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    let Some((db, db_url)) = postgres_fixture_and_url()? else {
        return Ok(None);
    };
    setup(db_url.clone()).context("failed to run Postgres test database setup")?;
    let pool = rt
        .block_on(establish_pool(db_url.as_str()))
        .context("failed to establish Postgres connection pool")?;
    Ok(Some(postgres_test_db(pool, db)))
}

macro_rules! dispatch_by_backend {
    ($sqlite_expr:expr, $postgres_expr:expr, $fallback:block) => {{
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        {
            return $sqlite_expr;
        }

        #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
        {
            return $postgres_expr;
        }

        #[cfg(not(any(feature = "sqlite", feature = "postgres")))]
        $fallback
    }};
}

/// Build a test database, returning `None` when the backend is unavailable.
///
/// # Errors
///
/// Returns any error raised while creating the database or connection pool.
pub async fn build_test_db_async(setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    dispatch_by_backend!(
        build_sqlite_test_db_async(setup).await,
        build_postgres_test_db_async(setup).await,
        {
            let _ = setup;
            Ok(None)
        }
    )
}

/// Build a test database, returning `None` when the backend is unavailable.
///
/// # Errors
///
/// Returns any error raised while creating the database or connection pool.
pub fn build_test_db(rt: &Runtime, setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    dispatch_by_backend!(
        build_sqlite_test_db(rt, setup),
        build_postgres_test_db(rt, setup),
        {
            let _ = (rt, setup);
            Ok(None)
        }
    )
}
