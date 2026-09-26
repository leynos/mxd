//! Shared permission seeding and `PostgreSQL` harness utilities.

#[cfg(feature = "postgres")]
use diesel_async::AsyncConnection;
use test_util::AnyError;

use crate::db::{DbConnection, download_file_permission, seed_permission};

/// Seed the canonical `download_file` permission and return its ID.
///
/// # Errors
///
/// Propagates any database error.
pub(crate) async fn seed_download_permission(conn: &mut DbConnection) -> Result<i32, AnyError> {
    seed_permission(conn, &download_file_permission())
        .await
        .map_err(anyhow::Error::from)
}

/// Run `f` against a freshly migrated database in an embedded `PostgreSQL`
/// cluster.
///
/// # Errors
///
/// Returns any setup, migration, or closure error.
#[cfg(feature = "postgres")]
pub(crate) async fn with_embedded_pg<F>(f: F) -> Result<(), AnyError>
where
    F: for<'conn> FnOnce(
        &'conn mut DbConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), AnyError>> + 'conn>,
    >,
{
    let db = test_util::postgres::PostgresTestDb::new_async()
        .await
        .map_err(anyhow::Error::from)?;
    crate::db::run_migrations(db.url.as_ref(), None)
        .await
        .map_err(anyhow::Error::from)?;
    let mut conn = diesel_async::AsyncPgConnection::establish(db.url.as_ref())
        .await
        .map_err(anyhow::Error::from)?;
    f(&mut conn).await
}
