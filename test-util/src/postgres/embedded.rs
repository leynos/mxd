//! Embedded `PostgreSQL` cluster lifecycle helpers for integration tests.

use std::error::Error as StdError;

use eyre::eyre;
use pg_embedded_setup_unpriv::{
    BootstrapResult as PgBootstrapResult,
    ClusterGuard,
    TestCluster,
    test_support::shared_cluster_handle,
};

use super::common::{
    DatabaseName,
    DatabaseUrl,
    drop_database,
    generate_db_name,
    migration_template_name,
};

pub(crate) struct EmbeddedPg {
    pub(crate) url: DatabaseUrl,
    pub(crate) db_name: DatabaseName,
    admin_url: DatabaseUrl,
    _guard: Option<ClusterGuard>,
}

impl Drop for EmbeddedPg {
    fn drop(&mut self) { drop_database(&self.admin_url, &self.db_name) }
}

/// Error type distinguishing `PostgreSQL` unavailability from initialization failures.
#[derive(Debug)]
pub(crate) enum EmbeddedPgError {
    /// `PostgreSQL` binary not found or cannot be started (unavailable).
    Unavailable(String),
    /// `PostgreSQL` started but initialization failed (genuine error).
    InitFailed(Box<dyn StdError + Send + Sync>),
}

impl std::fmt::Display for EmbeddedPgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(msg) => write!(f, "PostgreSQL unavailable: {msg}"),
            Self::InitFailed(e) => write!(f, "PostgreSQL initialization failed: {e}"),
        }
    }
}

impl StdError for EmbeddedPgError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Unavailable(_) => None,
            Self::InitFailed(e) => Some(&**e),
        }
    }
}

/// Starts an embedded `PostgreSQL` cluster with optional template support.
///
/// When `use_template` is true:
/// - First test creates a template database and runs migrations
/// - Subsequent tests clone from template (10-50ms vs 2-10s)
/// - Template name is based on migration directory content hash
///
/// When `use_template` is false:
/// - Each test gets a fresh database with migrations
/// - Traditional approach, slower but fully isolated
pub(crate) fn start_embedded_postgres_with_strategy<F>(
    setup: F,
    use_template: bool,
) -> Result<EmbeddedPg, EmbeddedPgError>
where
    F: FnOnce(&DatabaseUrl) -> Result<(), Box<dyn StdError + Send + Sync>>,
{
    if use_template {
        let handle = shared_cluster_handle().map_err(|e| {
            EmbeddedPgError::Unavailable(format!("bootstrapping shared embedded PostgreSQL: {e}"))
        })?;
        let connection = handle.connection();
        let admin_url = DatabaseUrl::parse(&connection.database_url("postgres"))
            .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
        let db_name =
            generate_db_name("test_").map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;

        // Template-based database creation uses per-template locking in the
        // shared handle, so migration setup only runs once per process.
        let template_name = migration_template_name().map_err(EmbeddedPgError::InitFailed)?;
        handle
            .ensure_template_exists(
                template_name.as_ref(),
                |template_db| -> PgBootstrapResult<()> {
                    let template_url = DatabaseUrl::parse(&connection.database_url(template_db))
                        .map_err(|e| eyre!("failed to parse template URL: {e}"))?;
                    setup(&template_url).map_err(|e| eyre!("migration failed: {e}"))?;
                    Ok(())
                },
            )
            .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
        handle
            .create_database_from_template(db_name.as_ref(), template_name.as_ref())
            .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
        let url = DatabaseUrl::parse(&connection.database_url(db_name.as_ref()))
            .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;

        return Ok(EmbeddedPg {
            url,
            db_name,
            admin_url,
            _guard: None,
        });
    }

    let (handle, guard) = TestCluster::new_split().map_err(|e| {
        EmbeddedPgError::Unavailable(format!("bootstrapping embedded PostgreSQL: {e}"))
    })?;
    let connection = handle.connection();
    let admin_url = DatabaseUrl::parse(&connection.database_url("postgres"))
        .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
    let db_name =
        generate_db_name("test_").map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
    handle
        .create_database(db_name.as_ref())
        .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
    let url = DatabaseUrl::parse(&connection.database_url(db_name.as_ref()))
        .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
    setup(&url).map_err(EmbeddedPgError::InitFailed)?;

    Ok(EmbeddedPg {
        url,
        db_name,
        admin_url,
        _guard: Some(guard),
    })
}

/// Starts an embedded `PostgreSQL` cluster (backward compatibility wrapper).
///
/// This function maintains backward compatibility by using the traditional
/// approach (no templates). For faster tests, use `start_embedded_postgres_with_strategy`
/// with `use_template = true`.
pub(crate) fn start_embedded_postgres<F>(setup: F) -> Result<EmbeddedPg, EmbeddedPgError>
where
    F: FnOnce(&DatabaseUrl) -> Result<(), Box<dyn StdError + Send + Sync>>,
{
    start_embedded_postgres_with_strategy(setup, false)
}

/// Starts an embedded `PostgreSQL` cluster on the caller's async runtime.
///
/// This variant avoids creating a nested runtime and is therefore safe to call
/// from async contexts.
pub(crate) async fn start_embedded_postgres_async<F>(
    setup: F,
) -> Result<EmbeddedPg, EmbeddedPgError>
where
    F: FnOnce(&DatabaseUrl) -> Result<(), Box<dyn StdError + Send + Sync>> + Send + 'static,
{
    let (handle, guard) = TestCluster::start_async_split().await.map_err(|e| {
        EmbeddedPgError::Unavailable(format!("bootstrapping embedded PostgreSQL: {e}"))
    })?;
    let admin_url = DatabaseUrl::parse(&handle.connection().database_url("postgres"))
        .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
    let (url, db_name) = tokio::task::spawn_blocking(move || {
        let db_name =
            generate_db_name("test_").map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
        handle
            .create_database(db_name.as_ref())
            .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
        let url = DatabaseUrl::parse(&handle.connection().database_url(db_name.as_ref()))
            .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
        setup(&url).map_err(EmbeddedPgError::InitFailed)?;
        Ok::<(DatabaseUrl, DatabaseName), EmbeddedPgError>((url, db_name))
    })
    .await
    .map_err(|error| EmbeddedPgError::InitFailed(Box::new(error)))??;
    Ok(EmbeddedPg {
        url,
        db_name,
        admin_url,
        _guard: Some(guard),
    })
}
