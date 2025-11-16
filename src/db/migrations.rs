//! Embedded migration utilities.

use cfg_if::cfg_if;
use diesel::result::QueryResult;
use diesel_migrations::MigrationHarness;

use super::connection::{DbConnection, MIGRATIONS};

cfg_if! {
    if #[cfg(feature = "sqlite")] {
        /// Run embedded database migrations.
        ///
        /// # Errors
        /// Returns any error produced by Diesel while running migrations.
        #[must_use = "handle the result"]
        pub async fn run_migrations(conn: &mut DbConnection) -> QueryResult<()> {
            use diesel::result::Error as DieselError;
            conn.spawn_blocking(|c| {
                c.run_pending_migrations(MIGRATIONS)
                    .map(|_| ())
                    .map_err(|e: Box<dyn std::error::Error + Send + Sync>| {
                        DieselError::QueryBuilderError(Box::new(std::io::Error::other(
                            e.to_string(),
                        )))
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
            use diesel::Connection;
            use diesel::result::Error as DieselError;
            let url = database_url.to_owned();
            tokio::task::spawn_blocking(move || -> QueryResult<()> {
                let mut conn = PgConnection::establish(&url)
                    .map_err(|e| DieselError::QueryBuilderError(Box::new(e)))?;
                conn.run_pending_migrations(MIGRATIONS)
                    .map(|_| ())
                    .map_err(|e: Box<dyn std::error::Error + Send + Sync>| {
                        DieselError::QueryBuilderError(Box::new(std::io::Error::other(
                            e.to_string(),
                        )))
                    })
            })
            .await
            .map_err(|e| {
                DieselError::QueryBuilderError(Box::new(std::io::Error::other(e.to_string())))
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
pub async fn apply_migrations(conn: &mut DbConnection, url: &str) -> QueryResult<()> {
    let _ = url;
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
