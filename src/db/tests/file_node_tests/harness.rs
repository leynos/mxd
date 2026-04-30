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

/// Run `f` against a freshly migrated `PostgreSQL` database.
///
/// Uses `POSTGRES_TEST_URL` when set; otherwise starts embedded `PostgreSQL`.
///
/// # Errors
///
/// Returns any setup, migration, closure, or shutdown error.
#[cfg(feature = "postgres")]
pub(crate) async fn with_embedded_pg<F>(db_name: &str, f: F) -> Result<(), AnyError>
where
    F: for<'conn> FnOnce(
        &'conn mut DbConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), AnyError>> + 'conn>,
    >,
{
    use postgresql_embedded::PostgreSQL;

    if std::env::var_os("POSTGRES_TEST_URL").is_some() {
        let db = test_util::postgres::PostgresTestDb::new_async()
            .await
            .map_err(anyhow::Error::from)?;
        crate::db::run_migrations(db.url.as_ref(), None)
            .await
            .map_err(anyhow::Error::from)?;
        let mut conn = diesel_async::AsyncPgConnection::establish(db.url.as_ref())
            .await
            .map_err(anyhow::Error::from)?;
        return f(&mut conn).await;
    }

    let mut pg = PostgreSQL::default();
    pg.setup().await.map_err(anyhow::Error::from)?;
    pg.start().await.map_err(anyhow::Error::from)?;

    let result = async {
        pg.create_database(db_name)
            .await
            .map_err(anyhow::Error::from)?;
        let url = pg.settings().url(db_name);
        crate::db::run_migrations(&url, None)
            .await
            .map_err(anyhow::Error::from)?;
        let mut conn = diesel_async::AsyncPgConnection::establish(&url)
            .await
            .map_err(anyhow::Error::from)?;
        f(&mut conn).await
    }
    .await;

    let stop_result = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(anyhow::Error::from)?;
        runtime.block_on(pg.stop()).map_err(anyhow::Error::from)
    })
    .join()
    .map_err(|_| anyhow::anyhow!("embedded postgres shutdown thread panicked"))?;
    match (result, stop_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(scenario_error), Ok(())) => Err(scenario_error),
        (Ok(()), Err(stop_error)) => Err(stop_error),
        (Err(scenario_error), Err(stop_error)) => Err(anyhow::anyhow!(
            "postgres scenario failed: {scenario_error}; embedded postgres shutdown failed: \
             {stop_error}"
        )),
    }
}
