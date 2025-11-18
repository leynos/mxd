//! Embedded migration utilities.

use std::{error::Error as StdError, fmt};

use cfg_if::cfg_if;
use diesel::result::{Error as DieselError, QueryResult};
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use diesel::{Connection, result::ConnectionError};
use diesel_migrations::MigrationHarness;

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

cfg_if! {
    if #[cfg(feature = "sqlite")] {
        /// Run embedded database migrations.
        ///
        /// # Errors
        /// Returns any error produced by Diesel while running migrations.
        #[must_use = "handle the result"]
        pub async fn run_migrations(conn: &mut DbConnection) -> QueryResult<()> {
            conn.spawn_blocking(|c| {
                c.run_pending_migrations(MIGRATIONS)
                    .map(|_| ())
                    .map_err(|e: Box<dyn StdError + Send + Sync>| {
                        DieselError::SerializationError(Box::new(MigrationHarnessError(e)))
                    })
            })
            .await?;
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
            task::spawn_blocking(move || -> QueryResult<()> {
                let mut conn = PgConnection::establish(&url).map_err(|e| {
                    DieselError::SerializationError(Box::new(MigrationConnectionError(e)))
                })?;
                conn.run_pending_migrations(MIGRATIONS)
                    .map(|_| ())
                    .map_err(|e: Box<dyn StdError + Send + Sync>| {
                        DieselError::SerializationError(Box::new(MigrationHarnessError(e)))
                    })
            })
            .await
            .map_err(|e| {
                DieselError::SerializationError(Box::new(MigrationExecutorError(e)))
            })?
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
