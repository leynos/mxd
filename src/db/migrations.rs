//! Embedded migration utilities.

use std::{error::Error as StdError, fmt, time::Duration};

use cfg_if::cfg_if;
use diesel::result::{Error as DieselError, QueryResult};
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use diesel::{Connection, result::ConnectionError};
use diesel_migrations::MigrationHarness;
use tokio::time::timeout;
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

const MIGRATION_TIMEOUT: Duration = Duration::from_secs(5);

/// Wrap a migration harness error in a Diesel error.
fn wrap_harness_error(e: Box<dyn StdError + Send + Sync>) -> DieselError {
    DieselError::SerializationError(Box::new(MigrationHarnessError(e)))
}

/// Wrap a timeout error in a Diesel error.
fn wrap_timeout_error() -> DieselError {
    DieselError::SerializationError(Box::new(MigrationTimeoutError(MIGRATION_TIMEOUT)))
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
fn apply_pending_migrations<C>(conn: &mut C) -> QueryResult<()>
where
    C: MigrationHarness<super::connection::Backend>,
{
    info!("applying pending migrations");
    conn.run_pending_migrations(MIGRATIONS)
        .map(|_| ())
        .map_err(wrap_harness_error)
}

/// Check for pending migrations and execute them if present.
///
/// Returns `Ok(())` if no migrations are pending or if migrations complete successfully.
fn execute_migrations_sync<C>(conn: &mut C) -> QueryResult<()>
where
    C: MigrationHarness<super::connection::Backend>,
{
    if !has_pending_migrations(conn) {
        info!("no pending migrations; skipping apply");
        return Ok(());
    }
    apply_pending_migrations(conn)
}

cfg_if! {
    if #[cfg(feature = "sqlite")] {
        /// Run embedded database migrations.
        ///
        /// # Errors
        /// Returns any error produced by Diesel while running migrations.
        #[must_use = "handle the result"]
        pub async fn run_migrations(conn: &mut DbConnection) -> QueryResult<()> {
            timeout(
                MIGRATION_TIMEOUT,
                conn.spawn_blocking(execute_migrations_sync),
            )
            .await
            .map_err(|_| wrap_timeout_error())??;
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

        /// Establish a PostgreSQL connection and execute migrations.
        fn establish_and_migrate(url: String) -> QueryResult<()> {
            use diesel::pg::PgConnection;
            let mut conn = PgConnection::establish(&url).map_err(wrap_connection_error)?;
            execute_migrations_sync(&mut conn)
        }

        /// Run embedded database migrations.
        ///
        /// # Errors
        /// Returns any error produced by Diesel while running migrations.
        #[must_use = "handle the result"]
        pub async fn run_migrations(database_url: &str) -> QueryResult<()> {
            use tokio::task;
            let url = database_url.to_owned();
            timeout(
                MIGRATION_TIMEOUT,
                task::spawn_blocking(move || establish_and_migrate(url)),
            )
            .await
            .map_err(|_| wrap_timeout_error())?
            .map_err(wrap_executor_error)??;
            Ok(())
        }
    }
}

/// Apply embedded migrations for the current backend.
///
/// # Errors
/// Returns any error produced by Diesel while running migrations.
#[cfg(feature = "sqlite")]
#[must_use = "handle the result"]
pub async fn apply_migrations(conn: &mut DbConnection, _database_url: &str) -> QueryResult<()> {
    run_migrations(conn).await
}

/// Apply embedded migrations for the current backend.
///
/// # Errors
/// Returns any error produced by Diesel while running migrations.
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
#[must_use = "handle the result"]
pub async fn apply_migrations(conn: &mut DbConnection, url: &str) -> QueryResult<()> {
    let _ = conn;
    run_migrations(url).await
}
