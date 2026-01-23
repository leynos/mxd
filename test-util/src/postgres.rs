//! Helpers for `PostgreSQL`-backed integration tests.

// Note: The rstest #[fixture] macro generates sibling items that cannot be
// individually annotated, requiring this module-level suppression.
#![expect(
    missing_docs,
    reason = "rstest #[fixture] macro generates undocumented helper items"
)]
use std::{
    error::Error as StdError,
    net::{TcpStream, ToSocketAddrs},
    ops::Deref,
    time::Duration,
};

use eyre::eyre;
use pg_embedded_setup_unpriv::{
    BootstrapResult as PgBootstrapResult,
    TestCluster,
    test_support::hash_directory,
};
use postgres::{Client, NoTls};
use rstest::fixture;
use url::Url;
use uuid::Uuid;

/// A validated `PostgreSQL` database connection URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseUrl(String);

/// A validated `PostgreSQL` database name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseName(String);

/// Validation error for [`DatabaseName::new`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DatabaseNameError {
    /// Database name is empty or contains only whitespace.
    #[error("database name cannot be empty")]
    Empty,
    /// Database name exceeds `PostgreSQL`'s 63-character limit.
    #[error("database name cannot exceed 63 characters")]
    TooLong,
    /// Database name contains non-ASCII-alphanumeric characters (except underscores).
    #[error("database name contains invalid characters")]
    InvalidCharacters,
}

impl DatabaseUrl {
    /// Parses a database URL string into a validated `DatabaseUrl`.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL cannot be parsed.
    pub fn parse(url: &str) -> Result<Self, url::ParseError> {
        Url::parse(url)?;
        Ok(Self(url.to_owned()))
    }
}

impl Deref for DatabaseUrl {
    type Target = str;
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl AsRef<str> for DatabaseUrl {
    fn as_ref(&self) -> &str { &self.0 }
}

impl std::fmt::Display for DatabaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) }
}

impl Deref for DatabaseName {
    type Target = str;
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl AsRef<str> for DatabaseName {
    fn as_ref(&self) -> &str { &self.0 }
}

impl std::fmt::Display for DatabaseName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) }
}

impl DatabaseName {
    /// Creates a new validated database name.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is empty, exceeds 63 characters, or
    /// contains non-alphanumeric characters (except underscores).
    #[expect(clippy::shadow_reuse, reason = "standard Into pattern")]
    pub fn new(name: impl Into<String>) -> Result<Self, DatabaseNameError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(DatabaseNameError::Empty);
        }
        if name.len() > 63 {
            return Err(DatabaseNameError::TooLong);
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(DatabaseNameError::InvalidCharacters);
        }
        Ok(Self(name))
    }
}

pub(crate) struct EmbeddedPg {
    url: DatabaseUrl,
    db_name: DatabaseName,
    admin_url: DatabaseUrl,
    _cluster: TestCluster,
}

/// Error indicating that a `PostgreSQL` server could not be reached.
#[derive(Debug)]
pub struct PostgresUnavailable;

impl std::fmt::Display for PostgresUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PostgreSQL server unreachable")
    }
}

impl std::error::Error for PostgresUnavailable {}

/// Error type for [`PostgresTestDb::new`].
#[derive(Debug)]
pub enum PostgresTestDbError {
    /// `PostgreSQL` binary not found or server unreachable.
    Unavailable(PostgresUnavailable),
    /// Initialization failed (URL parsing, database creation, etc.).
    InitFailed(Box<dyn StdError + Send + Sync>),
}

impl std::fmt::Display for PostgresTestDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(e) => write!(f, "{e}"),
            Self::InitFailed(e) => write!(f, "PostgreSQL initialization failed: {e}"),
        }
    }
}

impl StdError for PostgresTestDbError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Unavailable(e) => Some(e),
            Self::InitFailed(e) => Some(&**e),
        }
    }
}

impl From<PostgresUnavailable> for PostgresTestDbError {
    fn from(e: PostgresUnavailable) -> Self { Self::Unavailable(e) }
}

impl PostgresTestDbError {
    /// Returns `true` if this error indicates `PostgreSQL` is unavailable.
    #[must_use]
    pub const fn is_unavailable(&self) -> bool { matches!(self, Self::Unavailable(_)) }
}

#[expect(clippy::shadow_reuse, reason = "clearer flow with shadowing")]
fn postgres_available(url: &Url) -> bool {
    if let (Some(host), Some(port)) = (url.host_str(), url.port_or_known_default()) {
        let addr = (host, port)
            .to_socket_addrs()
            .ok()
            .and_then(|mut a| a.next());
        if let Some(addr) = addr {
            return TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_ok();
        }
    }
    false
}

fn generate_db_name(prefix: &str) -> Result<DatabaseName, DatabaseNameError> {
    let name = format!("{prefix}{}", Uuid::now_v7().simple());
    DatabaseName::new(name)
}

/// Generates a stable template name based on migration content hash.
/// Template name changes when migrations change, forcing template recreation.
fn migration_template_name() -> Result<DatabaseName, Box<dyn StdError + Send + Sync>> {
    let hash = hash_directory("migrations")?;
    // Use first 8 chars of hash for a reasonably unique but readable name.
    // The hash is always hexadecimal ASCII, so char boundary indexing is safe.
    let prefix = hash.get(..8).unwrap_or(&hash);
    DatabaseName::new(format!("template_{prefix}")).map_err(|e| Box::new(e) as _)
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
    let cluster = TestCluster::new().map_err(|e| {
        // TestCluster::new fails when the PostgreSQL binary is not found or cannot start.
        // This is an "unavailable" condition, not an initialization failure.
        EmbeddedPgError::Unavailable(format!("bootstrapping embedded PostgreSQL: {e}"))
    })?;
    let connection = cluster.connection();
    let admin_url = DatabaseUrl::parse(&connection.database_url("postgres"))
        .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;

    let db_name =
        generate_db_name("test_").map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;

    let url = if use_template {
        // Template-based database creation using ensure_template_exists for
        // concurrency-safe template creation with per-template locking.
        let template_name = migration_template_name().map_err(EmbeddedPgError::InitFailed)?;

        // ensure_template_exists handles the race condition by using per-template
        // locking - only one caller creates the template while others wait.
        cluster
            .ensure_template_exists(
                template_name.as_ref(),
                |template_db| -> PgBootstrapResult<()> {
                    // Run migrations on the newly created template database
                    let template_url = DatabaseUrl::parse(&connection.database_url(template_db))
                        .map_err(|e| eyre!("failed to parse template URL: {e}"))?;
                    setup(&template_url).map_err(|e| eyre!("migration failed: {e}"))?;
                    Ok(())
                },
            )
            .map_err(|e| EmbeddedPgError::InitFailed(Box::new(e)))?;

        // Clone from template
        create_db_from_template(&admin_url, &db_name, &template_name)
            .map_err(EmbeddedPgError::InitFailed)?
    } else {
        // Traditional approach: create fresh database and run migrations
        let (url, _) = create_external_db(&admin_url).map_err(EmbeddedPgError::InitFailed)?;
        setup(&url).map_err(EmbeddedPgError::InitFailed)?;
        url
    };

    Ok(EmbeddedPg {
        url,
        db_name,
        admin_url,
        _cluster: cluster,
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

pub(crate) fn reset_postgres_db(url: &DatabaseUrl) -> Result<(), Box<dyn StdError + Send + Sync>> {
    let mut client = Client::connect(url.as_ref(), NoTls)?;
    client.batch_execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")?;
    Ok(())
}

#[expect(
    clippy::print_stderr,
    reason = "test cleanup: user should see cleanup failures"
)]
fn drop_database(admin_url: &DatabaseUrl, db_name: &DatabaseName) {
    if let Ok(mut client) = Client::connect(admin_url.as_ref(), NoTls) {
        let query = format!("DROP DATABASE IF EXISTS \"{db_name}\"");
        if let Err(e) = client.batch_execute(&query) {
            eprintln!("error dropping database {db_name}: {e}");
        }
    }
}

/// Creates a database from a template.
fn create_db_from_template(
    admin_url: &DatabaseUrl,
    db_name: &DatabaseName,
    template_name: &DatabaseName,
) -> Result<DatabaseUrl, Box<dyn StdError + Send + Sync>> {
    let mut url = Url::parse(admin_url.as_ref())?;
    let admin_url_str = url.to_string();
    let mut client = Client::connect(&admin_url_str, NoTls)?;
    let query = format!(
        "CREATE DATABASE \"{}\" WITH TEMPLATE \"{}\"",
        db_name.as_ref(),
        template_name.as_ref()
    );
    client.batch_execute(&query)?;
    url.set_path(db_name.as_ref());
    Ok(DatabaseUrl::parse(url.as_str())?)
}

fn create_external_db(
    base_url: &DatabaseUrl,
) -> Result<(DatabaseUrl, DatabaseName), Box<dyn StdError + Send + Sync>> {
    let mut url = Url::parse(base_url.as_ref())?;
    let db_name = generate_db_name("test_")?;
    let admin_url = url.to_string();
    let mut client = Client::connect(&admin_url, NoTls)?;
    let query = format!("CREATE DATABASE \"{db_name}\"");
    client.batch_execute(&query)?;
    url.set_path(&db_name);
    Ok((DatabaseUrl::parse(url.as_str())?, db_name))
}

fn drop_external_db(admin_url: &DatabaseUrl, db_name: &DatabaseName) {
    drop_database(admin_url, db_name);
}

/// A test database instance backed by either embedded or external `PostgreSQL`.
pub struct PostgresTestDb {
    /// The connection URL for the test database.
    pub url: DatabaseUrl,
    admin_url: Option<DatabaseUrl>,
    embedded: Option<EmbeddedPg>,
    db_name: Option<DatabaseName>,
}

impl PostgresTestDb {
    /// Creates a new test database instance.
    ///
    /// Uses `POSTGRES_TEST_URL` environment variable if set, otherwise starts
    /// an embedded `PostgreSQL` instance.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresTestDbError::Unavailable`] if:
    /// - The configured external server cannot be reached
    /// - The embedded `PostgreSQL` binary is not available
    ///
    /// Returns [`PostgresTestDbError::InitFailed`] for other errors
    /// (URL parsing, database creation, etc.).
    pub fn new() -> Result<Self, PostgresTestDbError> {
        if let Some(value) = std::env::var_os("POSTGRES_TEST_URL") {
            let admin_url = DatabaseUrl::parse(&value.to_string_lossy())
                .map_err(|e| PostgresTestDbError::InitFailed(Box::new(e)))?;
            let parsed = Url::parse(admin_url.as_ref())
                .map_err(|e| PostgresTestDbError::InitFailed(Box::new(e)))?;
            if !postgres_available(&parsed) {
                return Err(PostgresTestDbError::Unavailable(PostgresUnavailable));
            }
            let (url, db_name) =
                create_external_db(&admin_url).map_err(PostgresTestDbError::InitFailed)?;
            return Ok(Self {
                url,
                admin_url: Some(admin_url),
                embedded: None,
                db_name: Some(db_name),
            });
        }

        let embedded = start_embedded_postgres(reset_postgres_db).map_err(|e| match e {
            EmbeddedPgError::Unavailable(_) => {
                PostgresTestDbError::Unavailable(PostgresUnavailable)
            }
            EmbeddedPgError::InitFailed(inner) => PostgresTestDbError::InitFailed(inner),
        })?;
        let url = embedded.url.clone();
        let db_name = embedded.db_name.clone();
        Ok(Self {
            url,
            admin_url: None,
            embedded: Some(embedded),
            db_name: Some(db_name),
        })
    }

    /// Creates a new test database instance using template-based cloning.
    ///
    /// This method provides significantly faster database creation by:
    /// - Creating a template database once (with migrations applied)
    /// - Cloning subsequent databases from the template (10-50ms vs 2-10s)
    ///
    /// Like [`Self::new`], uses `POSTGRES_TEST_URL` if set, otherwise embedded `PostgreSQL`.
    ///
    /// # Performance
    ///
    /// - First test: ~5-10 seconds (template creation + migration)
    /// - Subsequent tests: ~10-50ms (template clone only)
    ///
    /// # Errors
    ///
    /// Returns [`PostgresTestDbError::Unavailable`] if `PostgreSQL` is unreachable.
    /// Returns [`PostgresTestDbError::InitFailed`] for initialization errors.
    pub fn new_from_template() -> Result<Self, PostgresTestDbError> {
        if std::env::var_os("POSTGRES_TEST_URL").is_some() {
            // External PostgreSQL: template support not yet implemented for external servers
            // Fall back to regular database creation
            return Self::new();
        }

        let embedded = start_embedded_postgres_with_strategy(reset_postgres_db, true).map_err(
            |e| match e {
                EmbeddedPgError::Unavailable(_) => {
                    PostgresTestDbError::Unavailable(PostgresUnavailable)
                }
                EmbeddedPgError::InitFailed(inner) => PostgresTestDbError::InitFailed(inner),
            },
        )?;
        let url = embedded.url.clone();
        let db_name = embedded.db_name.clone();
        Ok(Self {
            url,
            admin_url: None,
            embedded: Some(embedded),
            db_name: Some(db_name),
        })
    }

    /// Returns `true` if this test database uses an embedded `PostgreSQL` instance.
    #[must_use]
    pub const fn uses_embedded(&self) -> bool { self.embedded.is_some() }
}

impl Drop for PostgresTestDb {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "best-effort cleanup; Drop cannot propagate errors"
    )]
    fn drop(&mut self) {
        if let Some(embedded) = self.embedded.take() {
            drop_database(&embedded.admin_url, &embedded.db_name);
            return;
        }
        match (&self.admin_url, &self.db_name) {
            (Some(admin), Some(name)) => drop_external_db(admin, name),
            _ => {
                let _ = reset_postgres_db(&self.url);
            }
        }
    }
}

/// rstest fixture providing a `PostgreSQL` test database.
///
/// This fixture is for tests that **require** `PostgreSQL` and should fail loudly
/// when it is unavailable. Use this when `PostgreSQL` is a hard dependency of the
/// test (e.g., testing `PostgreSQL`-specific SQL syntax).
///
/// # When to Use Direct `PostgresTestDb::new()` Instead
///
/// For tests that should **skip gracefully** when `PostgreSQL` is unavailable,
/// call `PostgresTestDb::new()` directly and check for
/// [`PostgresTestDbError::Unavailable`]:
///
/// ```ignore
/// let db = match PostgresTestDb::new() {
///     Ok(db) => db,
///     Err(PostgresTestDbError::Unavailable(_)) => {
///         eprintln!("PostgreSQL unavailable; skipping test");
///         return Ok(());
///     }
///     Err(e) => return Err(e.into()),
/// };
/// ```
///
/// # Panics
///
/// Panics if:
/// - `PostgreSQL` is unavailable (binary not found or server unreachable)
/// - Initialization or database creation fails (configuration error)
#[fixture]
pub fn postgres_db() -> PostgresTestDb {
    PostgresTestDb::new().unwrap_or_else(|e| {
        let msg = if e.is_unavailable() {
            format!("PostgreSQL unavailable: {e}")
        } else {
            format!("Failed to prepare Postgres test database: {e}")
        };
        panic!("{msg}");
    })
}

/// rstest fixture providing a fast `PostgreSQL` test database via template cloning.
///
/// This fixture uses database templating to provide sub-second test database
/// creation. A template database with migrations applied is created once per
/// test process and reused across all tests.
///
/// # Performance
///
/// - First test: ~5-10 seconds (template creation + migration)
/// - Subsequent tests: ~10-50ms (template clone only)
/// - 95-99% faster than `postgres_db` for large test suites
///
/// # Use Cases
///
/// - Large test suites where startup time is significant
/// - Tests that don't modify `PostgreSQL` cluster settings
/// - Tests that only need database-level isolation
///
/// # When to Use `postgres_db` Instead
///
/// - Tests that modify cluster-level `PostgreSQL` settings
/// - Tests that require full cluster isolation
/// - First-time setup or when debugging migration issues
///
/// # Panics
///
/// Panics if:
/// - `PostgreSQL` is unavailable (binary not found or server unreachable)
/// - Template creation or database cloning fails
#[fixture]
pub fn postgres_db_fast() -> PostgresTestDb {
    PostgresTestDb::new_from_template().unwrap_or_else(|e| {
        let msg = if e.is_unavailable() {
            format!("PostgreSQL unavailable: {e}")
        } else {
            format!("Failed to create PostgreSQL test database from template: {e}")
        };
        panic!("{msg}");
    })
}
