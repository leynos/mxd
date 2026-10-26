//! Embedded `PostgreSQL` cluster lifecycle helpers for integration tests.

use std::{error::Error as StdError, time::Duration};

use eyre::eyre;
use pg_embedded_setup_unpriv::{
    BootstrapResult as PgBootstrapResult,
    ClusterGuard,
    ClusterHandle,
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

/// How many times a cluster start is attempted when it loses a slot race.
///
/// `pg-embed-setup-unpriv` 0.6.3 sweeps dead cluster slots before taking the
/// guard that orders slot claims, and it creates a slot's lock file before
/// locking it. Two processes bootstrapping at once can therefore have the
/// second process's just-created lock locked by the first process's sweep,
/// which then removes the slot as dead. The loser sees
/// `slot lock ... is already held` and gives up, even though a fresh attempt
/// would claim a new, differently named slot.
const SLOT_CONTENTION_ATTEMPTS: u32 = 8;

/// The pause before retrying a lost slot race, doubling each attempt.
const SLOT_CONTENTION_BACKOFF: Duration = Duration::from_millis(25);

/// Renders an error together with every cause beneath it.
///
/// `pg_embed_setup_unpriv::BootstrapError` displays only its outermost
/// context, and that crate wraps the underlying `postgresql_embedded` failure
/// as a source rather than folding it into the message. Formatting with
/// `Display` alone therefore reports `postgresql_embedded::start() failed` and
/// discards the `pg_ctl` diagnostics that say why the server would not start.
fn describe_error_chain(error: &dyn StdError) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        message.push_str(": ");
        message.push_str(&source.to_string());
        cause = source.source();
    }
    message
}

/// Whether a failure is the lost slot race described on
/// [`SLOT_CONTENTION_ATTEMPTS`].
///
/// Matched on the message because the crate reports this failure as a plain
/// bootstrap error of kind `Other`: it has no variant of its own, so there is
/// nothing typed to inspect.
fn is_slot_contention(error: &impl StdError) -> bool {
    let message = describe_error_chain(error);
    message.contains("slot lock") && message.contains("is already held")
}

/// Decides what a failed cluster start means.
///
/// `Ok` carries how long to wait before the next attempt, so a lost slot race
/// costs another bootstrap. `Err` carries the failure to report, either
/// because the attempt budget is spent or because the failure is something
/// other than the lost slot race.
fn slot_contention_retry(attempt: u32, error: &impl StdError) -> Result<Duration, EmbeddedPgError> {
    if attempt < SLOT_CONTENTION_ATTEMPTS && is_slot_contention(error) {
        Ok(SLOT_CONTENTION_BACKOFF * attempt)
    } else {
        Err(EmbeddedPgError::BootstrapFailed(format!(
            "bootstrapping embedded PostgreSQL: {}",
            describe_error_chain(error)
        )))
    }
}

/// Starts a cluster of this database's own, retrying a lost slot race.
///
/// Each attempt is a whole bootstrap, because losing the race leaves nothing
/// usable behind: the sweeper has removed the slot the attempt had claimed.
fn start_cluster_retrying_slot_contention() -> Result<(ClusterHandle, ClusterGuard), EmbeddedPgError>
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match TestCluster::new_split() {
            Ok(split) => return Ok(split),
            Err(error) => std::thread::sleep(slot_contention_retry(attempt, &error)?),
        }
    }
}

pub(crate) struct EmbeddedPg {
    pub(crate) url: DatabaseUrl,
    pub(crate) db_name: DatabaseName,
    admin_url: DatabaseUrl,
    _guard: Option<ClusterGuard>,
}

impl Drop for EmbeddedPg {
    fn drop(&mut self) { drop_database(&self.admin_url, &self.db_name) }
}

/// Error type distinguishing a cluster that never started from one that did.
#[derive(Debug)]
pub(crate) enum EmbeddedPgError {
    /// The cluster could not be bootstrapped or started.
    BootstrapFailed(String),
    /// The cluster started but preparing the test database failed.
    InitFailed(Box<dyn StdError + Send + Sync>),
}

impl std::fmt::Display for EmbeddedPgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BootstrapFailed(msg) => write!(f, "PostgreSQL bootstrap failed: {msg}"),
            Self::InitFailed(e) => write!(f, "PostgreSQL initialization failed: {e}"),
        }
    }
}

impl StdError for EmbeddedPgError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::BootstrapFailed(_) => None,
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
        start_from_template(setup)
    } else {
        start_fresh_cluster(setup)
    }
}

/// Clone a test database from the migrated template in the shared cluster.
///
/// Template-based creation uses per-template locking in the shared handle, so
/// migration setup runs only once per process.
fn start_from_template<F>(setup: F) -> Result<EmbeddedPg, EmbeddedPgError>
where
    F: FnOnce(&DatabaseUrl) -> Result<(), Box<dyn StdError + Send + Sync>>,
{
    let handle = shared_cluster_handle().map_err(|e| {
        EmbeddedPgError::BootstrapFailed(format!("bootstrapping shared embedded PostgreSQL: {e}"))
    })?;
    let connection = handle.connection();
    let admin_url = parse_url(&connection.database_url("postgres"))?;
    let db_name =
        generate_db_name("test_").map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
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
    let url = parse_url(&connection.database_url(db_name.as_ref()))?;
    Ok(EmbeddedPg {
        url,
        db_name,
        admin_url,
        _guard: None,
    })
}

/// Start a cluster of this database's own, create the database and run setup.
fn start_fresh_cluster<F>(setup: F) -> Result<EmbeddedPg, EmbeddedPgError>
where
    F: FnOnce(&DatabaseUrl) -> Result<(), Box<dyn StdError + Send + Sync>>,
{
    let (handle, guard) = start_cluster_retrying_slot_contention()?;
    let connection = handle.connection();
    let admin_url = parse_url(&connection.database_url("postgres"))?;
    let db_name =
        generate_db_name("test_").map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
    handle
        .create_database(db_name.as_ref())
        .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;
    let url = parse_url(&connection.database_url(db_name.as_ref()))?;
    setup(&url).map_err(EmbeddedPgError::InitFailed)?;
    Ok(EmbeddedPg {
        url,
        db_name,
        admin_url,
        _guard: Some(guard),
    })
}

/// Async twin of [`start_cluster_retrying_slot_contention`].
///
/// The caller is already on a runtime, so the backoff yields rather than
/// blocking a worker thread.
async fn start_cluster_async_retrying_slot_contention()
-> Result<(ClusterHandle, ClusterGuard), EmbeddedPgError> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match TestCluster::start_async_split().await {
            Ok(split) => return Ok(split),
            Err(error) => tokio::time::sleep(slot_contention_retry(attempt, &error)?).await,
        }
    }
}

/// Parse a URL the cluster produced, as an initialization failure if invalid.
fn parse_url(url: &str) -> Result<DatabaseUrl, EmbeddedPgError> {
    DatabaseUrl::parse(url).map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))
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
    let (handle, guard) = start_cluster_async_retrying_slot_contention().await?;
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
