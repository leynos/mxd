//! Connection and pool helpers for database access.

use cfg_if::cfg_if;
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, PoolError, bb8::Pool};
#[cfg(feature = "sqlite")]
use diesel_async::sync_connection_wrapper::SyncConnectionWrapper;
use diesel_migrations::{EmbeddedMigrations, embed_migrations};

cfg_if! {
    if #[cfg(all(feature = "sqlite", feature = "postgres", not(feature = "lint")))] {
        compile_error!("Either feature 'sqlite' or 'postgres' must be enabled, not both");
    } else if #[cfg(feature = "sqlite")] {
        use diesel::sqlite::{Sqlite, SqliteConnection};
        pub type Backend = Sqlite;
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/sqlite");
        pub type DbConnection = SyncConnectionWrapper<SqliteConnection>;
        pub type DbPool = Pool<DbConnection>;
    } else if #[cfg(all(feature = "postgres", not(feature = "sqlite")))] {
        use diesel::pg::Pg;
        use diesel_async::AsyncPgConnection;
        pub type Backend = Pg;
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/postgres");
        pub type DbConnection = AsyncPgConnection;
        pub type DbPool = Pool<DbConnection>;
    } else {
        compile_error!("Either feature 'sqlite' or 'postgres' must be enabled");
    }
}

/// Create a pooled connection to the configured database.
///
/// Asynchronously establishes a database connection pool for the configured
/// backend, returning any pool initialisation failure to the caller.
///
/// # Examples
///
/// ```no_run
/// use mxd::db::establish_pool;
/// async fn example() {
///     let pool = establish_pool("sqlite::memory:")
///         .await
///         .expect("failed to build pool");
/// }
/// ```
///
/// # Errors
/// Returns any error reported by the underlying connection pool builder.
pub async fn establish_pool(database_url: &str) -> Result<DbPool, PoolError> {
    let config = AsyncDieselConnectionManager::<DbConnection>::new(database_url);
    Pool::builder().build(config).await
}
