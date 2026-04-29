//! Embedded migration utilities.

use std::{error::Error as StdError, fmt, future::Future, time::Duration};

use cfg_if::cfg_if;
use diesel::result::{Error as DieselError, QueryResult};
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use diesel::{Connection, result::ConnectionError};
use diesel_migrations::{MigrationError, MigrationHarness};
use futures_util::{FutureExt, future::Either, pin_mut};
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::connection::{DbConnection, MIGRATIONS};

#[derive(Debug)]
struct MigrationHarnessError(Box<dyn StdError + Send + Sync>);

impl fmt::Display for MigrationHarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "migration harness error: {}", self.0)
    }
}

impl StdError for MigrationHarnessError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> { Some(&*self.0) }
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
#[derive(Debug)]
struct MigrationExecutorError(tokio::task::JoinError);

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
impl fmt::Display for MigrationExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "migration executor error: {}", self.0)
    }
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
impl StdError for MigrationExecutorError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> { Some(&self.0) }
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
#[derive(Debug)]
struct MigrationConnectionError(ConnectionError);

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
impl fmt::Display for MigrationConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "migration connection error: {}", self.0)
    }
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
impl StdError for MigrationConnectionError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> { Some(&self.0) }
}

#[derive(Debug, Clone, Copy)]
struct MigrationTimeoutError(Duration);

impl fmt::Display for MigrationTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "migration execution exceeded {:?}", self.0)
    }
}

impl StdError for MigrationTimeoutError {}

#[derive(Debug)]
struct MigrationCancelledError;

impl fmt::Display for MigrationCancelledError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("migration execution cancelled")
    }
}

impl StdError for MigrationCancelledError {}

// The additive file-node migration increases setup cost enough that the old
// five-second cap becomes flaky under nextest's parallel SQLite database
// creation. Keep the watchdog, but give embedded migrations enough headroom to
// finish deterministically on loaded CI and developer machines.
const DEFAULT_MIGRATION_TIMEOUT: Duration = Duration::from_secs(15);
const MIGRATION_CANCEL_GRACE_TIMEOUT: Duration = Duration::from_secs(1);

/// Wrap a migration harness error in a Diesel error.
fn wrap_harness_error(e: Box<dyn StdError + Send + Sync>) -> DieselError {
    DieselError::SerializationError(Box::new(MigrationHarnessError(e)))
}

/// Wrap a timeout error in a Diesel error.
fn wrap_timeout_error(duration: Duration) -> DieselError {
    DieselError::SerializationError(Box::new(MigrationTimeoutError(duration)))
}

fn wrap_cancellation_error() -> DieselError {
    DieselError::SerializationError(Box::new(MigrationCancelledError))
}

fn migration_timeout(timeout_secs: Option<u64>) -> Duration {
    timeout_secs
        .filter(|seconds| *seconds > 0)
        .map_or(DEFAULT_MIGRATION_TIMEOUT, Duration::from_secs)
}

fn log_migration_completed_within_cancellation_grace() {
    info!(
        grace_timeout_secs = MIGRATION_CANCEL_GRACE_TIMEOUT.as_secs(),
        "migration completed within cancellation grace period; returning timeout error"
    );
}

fn log_migration_overran_cancellation_grace() {
    info!(
        grace_timeout_secs = MIGRATION_CANCEL_GRACE_TIMEOUT.as_secs(),
        "migration overran cancellation grace period; returning timeout error"
    );
}

async fn migration_completed_within_cancellation_grace<F, T>(pending_migration: F) -> bool
where
    F: Future<Output = T>,
{
    tokio::time::timeout(MIGRATION_CANCEL_GRACE_TIMEOUT, pending_migration)
        .await
        .is_ok()
}

async fn log_cancellation_grace_result<F, T>(pending_migration: F)
where
    F: Future<Output = T>,
{
    if migration_completed_within_cancellation_grace(pending_migration).await {
        log_migration_completed_within_cancellation_grace();
    } else {
        log_migration_overran_cancellation_grace();
    }
}

async fn cancel_timed_out_migration<F, T>(
    duration: Duration,
    token: CancellationToken,
    pending_migration: F,
) -> Result<T, DieselError>
where
    F: Future<Output = T>,
{
    info!(
        timeout_secs = duration.as_secs(),
        "migration watchdog fired; cancelling in-progress work"
    );
    token.cancel();
    log_cancellation_grace_result(pending_migration).await;
    Err(wrap_timeout_error(duration))
}

async fn run_with_migration_timeout<F, T>(
    duration: Duration,
    token: CancellationToken,
    future: F,
) -> Result<T, DieselError>
where
    F: Future<Output = T>,
{
    let migration_future = future.fuse();
    let timeout_sleep = tokio::time::sleep(duration).fuse();
    pin_mut!(migration_future);
    pin_mut!(timeout_sleep);

    match futures_util::future::select(migration_future, timeout_sleep).await {
        Either::Left((result, _)) => Ok(result),
        Either::Right(((), pending_migration)) => {
            cancel_timed_out_migration(duration, token, pending_migration).await
        }
    }
}

/// Check whether migrations are pending.
///
/// Returns `true` if there are pending migrations, `false` otherwise.
fn has_pending_migrations<C>(conn: &mut C) -> bool
where
    C: MigrationHarness<super::connection::Backend>,
{
    // has_pending_migration returns Ok(true) if pending, Ok(false) if not,
    // or Err if it cannot determine. Treat errors as "pending" to be safe.
    !matches!(conn.has_pending_migration(MIGRATIONS), Ok(false))
}

/// Execute all pending migrations.
///
/// Assumes caller has already verified migrations are pending.
fn ensure_migrations_not_cancelled(token: &CancellationToken) -> QueryResult<()> {
    if token.is_cancelled() {
        Err(wrap_cancellation_error())
    } else {
        Ok(())
    }
}

fn is_no_migration_run_error(error: &(dyn StdError + Send + Sync + 'static)) -> bool {
    error
        .downcast_ref::<MigrationError>()
        .is_some_and(|inner| matches!(inner, MigrationError::NoMigrationRun))
}

fn apply_pending_migrations<C>(conn: &mut C, token: &CancellationToken) -> QueryResult<()>
where
    C: MigrationHarness<super::connection::Backend>,
{
    info!("applying pending migrations");
    loop {
        ensure_migrations_not_cancelled(token)?;
        match conn.run_next_migration(MIGRATIONS) {
            Ok(_) => (),
            Err(error) if is_no_migration_run_error(&*error) => return Ok(()),
            Err(error) => return Err(wrap_harness_error(error)),
        }
    }
}

/// Check for pending migrations and execute them if present.
///
/// Returns `Ok(())` if no migrations are pending or if migrations complete successfully.
fn execute_migrations_sync<C>(conn: &mut C, token: &CancellationToken) -> QueryResult<()>
where
    C: MigrationHarness<super::connection::Backend>,
{
    if !has_pending_migrations(conn) {
        info!("no pending migrations; skipping apply");
        return Ok(());
    }
    apply_pending_migrations(conn, token)
}

cfg_if! {
    if #[cfg(feature = "sqlite")] {
        /// Run embedded database migrations.
        ///
        /// # Parameters
        ///
        /// - `timeout_secs`: optional watchdog duration, in seconds. `None` or `Some(0)`
        ///   uses the built-in default timeout.
        ///
        /// # Errors
        ///
        /// Returns any error produced by Diesel while running migrations.
        /// Returns a wrapped [`MigrationTimeoutError`] when the watchdog
        /// cancels work that exceeds `timeout_secs`. [`MigrationCancelledError`]
        /// is only returned if cancellation is observed separately inside the
        /// migration loop.
        #[must_use = "handle the result"]
        pub async fn run_migrations(
            conn: &mut DbConnection,
            timeout_secs: Option<u64>,
        ) -> QueryResult<()> {
            let timeout = migration_timeout(timeout_secs);
            let token = CancellationToken::new();
            let migration_token = token.clone();
            run_with_migration_timeout(
                timeout,
                token,
                conn.spawn_blocking(move |inner| execute_migrations_sync(inner, &migration_token)),
            )
                .await??;
            Ok(())
        }
    } else if #[cfg(all(feature = "postgres", not(feature = "sqlite")))] {
        /// Wrap a connection error in a Diesel error.
        fn wrap_connection_error(e: ConnectionError) -> DieselError {
            DieselError::SerializationError(Box::new(MigrationConnectionError(e)))
        }

        /// Wrap a task executor error in a Diesel error.
        fn wrap_executor_error(e: tokio::task::JoinError) -> DieselError {
            DieselError::SerializationError(Box::new(MigrationExecutorError(e)))
        }

        /// Establish a `PostgreSQL` connection and execute migrations.
        fn establish_and_migrate(url: &str, token: &CancellationToken) -> QueryResult<()> {
            use diesel::pg::PgConnection;
            let mut conn = PgConnection::establish(url).map_err(wrap_connection_error)?;
            execute_migrations_sync(&mut conn, token)
        }

        /// Run embedded database migrations.
        ///
        /// # Parameters
        ///
        /// - `timeout_secs`: optional watchdog duration, in seconds. `None` or `Some(0)`
        ///   uses the built-in default timeout.
        ///
        /// # Errors
        ///
        /// Returns any error produced by Diesel while running migrations.
        /// Returns a wrapped [`MigrationTimeoutError`] when the watchdog
        /// cancels work that exceeds `timeout_secs`. [`MigrationCancelledError`]
        /// is only returned if cancellation is observed separately inside the
        /// migration loop.
        #[must_use = "handle the result"]
        pub async fn run_migrations(
            database_url: &str,
            timeout_secs: Option<u64>,
        ) -> QueryResult<()> {
            use tokio::task;
            let url = database_url.to_owned();
            let timeout = migration_timeout(timeout_secs);
            let token = CancellationToken::new();
            let migration_token = token.clone();
            run_with_migration_timeout(
                timeout,
                token,
                task::spawn_blocking(move || establish_and_migrate(&url, &migration_token)),
            )
                .await?
                .map_err(wrap_executor_error)??;
            Ok(())
        }
    }
}

/// Apply embedded migrations for the current backend.
///
/// # Parameters
///
/// - `timeout_secs`: optional watchdog duration, in seconds. `None` or `Some(0)` uses the built-in
///   default timeout.
///
/// # Errors
///
/// Returns any error produced by Diesel while running migrations. Returns a
/// wrapped [`MigrationTimeoutError`] when the watchdog cancels work that
/// exceeds `timeout_secs`. [`MigrationCancelledError`] is only returned if
/// cancellation is observed separately inside the migration loop.
#[cfg(feature = "sqlite")]
#[must_use = "handle the result"]
pub async fn apply_migrations(
    conn: &mut DbConnection,
    _database_url: &str,
    timeout_secs: Option<u64>,
) -> QueryResult<()> {
    run_migrations(conn, timeout_secs).await
}

/// Apply embedded migrations for the current backend.
///
/// # Parameters
///
/// - `timeout_secs`: optional watchdog duration, in seconds. `None` or `Some(0)` uses the built-in
///   default timeout.
///
/// # Errors
///
/// Returns any error produced by Diesel while running migrations. Returns a
/// wrapped [`MigrationTimeoutError`] when the watchdog cancels work that
/// exceeds `timeout_secs`. [`MigrationCancelledError`] is only returned if
/// cancellation is observed separately inside the migration loop.
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
#[must_use = "handle the result"]
pub async fn apply_migrations(
    conn: &mut DbConnection,
    url: &str,
    timeout_secs: Option<u64>,
) -> QueryResult<()> {
    let _ = conn;
    run_migrations(url, timeout_secs).await
}

#[cfg(test)]
#[path = "migrations_tests.rs"]
mod tests;
