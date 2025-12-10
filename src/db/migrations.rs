//! Embedded migration utilities.

#![allow(
    clippy::cognitive_complexity,
    reason = "migration logic involves multiple conditional branches"
)]

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
                conn.spawn_blocking(|c| {
                    if let Ok(false) = c.has_pending_migration(MIGRATIONS) {
                        info!("no pending migrations; skipping apply");
                        return Ok(());
                    }
                    info!("applying pending migrations");
                    c.run_pending_migrations(MIGRATIONS)
                        .map(|_| ())
                        .map_err(|e: Box<dyn StdError + Send + Sync>| {
                            DieselError::SerializationError(Box::new(MigrationHarnessError(e)))
                        })
                }),
            )
            .await
            .map_err(|_| {
                DieselError::SerializationError(Box::new(MigrationTimeoutError(MIGRATION_TIMEOUT)))
            })??;
            Ok(())
        }
    } else if #[cfg(all(feature = "postgres", not(feature = "sqlite")))] {
        /// Run embedded database migrations.
        ///
        /// # Errors
        /// Returns any error produced by Diesel while running migrations.
        #[must_use = "handle the result"]
        pub async fn run_migrations(database_url: &str) -> QueryResult<()> {
            use diesel::pg::PgConnection;
            use tokio::task;
            let url = database_url.to_owned();
            timeout(
                MIGRATION_TIMEOUT,
                task::spawn_blocking(move || -> QueryResult<()> {
                    let mut conn = PgConnection::establish(&url).map_err(|e| {
                        DieselError::SerializationError(Box::new(MigrationConnectionError(e)))
                    })?;
                    if let Ok(false) = conn.has_pending_migration(MIGRATIONS) {
                        info!("no pending migrations; skipping apply");
                        return Ok(());
                    }
                    info!("applying pending migrations");
                    conn.run_pending_migrations(MIGRATIONS)
                        .map(|_| ())
                        .map_err(|e: Box<dyn StdError + Send + Sync>| {
                            DieselError::SerializationError(Box::new(MigrationHarnessError(e)))
                        })
                }),
            )
            .await
            .map_err(|_| {
                DieselError::SerializationError(Box::new(MigrationTimeoutError(MIGRATION_TIMEOUT)))
            })?
            .map_err(|e| DieselError::SerializationError(Box::new(MigrationExecutorError(e))))??;
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
