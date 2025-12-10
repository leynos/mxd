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

use pg_embedded_setup_unpriv::TestCluster;
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
    pub fn new(name: impl Into<String>) -> Result<Self, String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("database name cannot be empty".into());
        }
        if name.len() > 63 {
            return Err("database name cannot exceed 63 characters".into());
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err("database name contains invalid characters".into());
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

fn generate_db_name(prefix: &str) -> Result<DatabaseName, String> {
    let name = format!("{prefix}{}", Uuid::now_v7().simple());
    DatabaseName::new(name)
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

pub(crate) fn start_embedded_postgres<F>(setup: F) -> Result<EmbeddedPg, EmbeddedPgError>
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
    let (url, db_name) = create_external_db(&admin_url).map_err(EmbeddedPgError::InitFailed)?;
    setup(&url).map_err(EmbeddedPgError::InitFailed)?;
    Ok(EmbeddedPg {
        url,
        db_name,
        admin_url,
        _cluster: cluster,
    })
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
    /// Returns `PostgresUnavailable` if:
    /// - The configured external server cannot be reached
    /// - The embedded `PostgreSQL` binary is not available
    ///
    /// Propagates other errors (initialization, database creation) directly
    /// to distinguish them from unavailability conditions.
    pub fn new() -> Result<Self, Box<dyn StdError + Send + Sync>> {
        if let Some(value) = std::env::var_os("POSTGRES_TEST_URL") {
            let admin_url = DatabaseUrl::parse(&value.to_string_lossy())?;
            let parsed = Url::parse(admin_url.as_ref())?;
            if !postgres_available(&parsed) {
                return Err(Box::new(PostgresUnavailable));
            }
            let (url, db_name) = create_external_db(&admin_url)?;
            return Ok(Self {
                url,
                admin_url: Some(admin_url),
                embedded: None,
                db_name: Some(db_name),
            });
        }

        let embedded = start_embedded_postgres(reset_postgres_db).map_err(|e| match e {
            EmbeddedPgError::Unavailable(_) => {
                Box::new(PostgresUnavailable) as Box<dyn StdError + Send + Sync>
            }
            EmbeddedPgError::InitFailed(inner) => inner,
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
/// call `PostgresTestDb::new()` directly and check for `PostgresUnavailable`:
///
/// ```ignore
/// let db = match PostgresTestDb::new() {
///     Ok(db) => db,
///     Err(e) if e.is::<PostgresUnavailable>() => {
///         eprintln!("PostgreSQL unavailable; skipping test");
///         return Ok(());
///     }
///     Err(e) => return Err(e),
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
        let msg = if e.is::<PostgresUnavailable>() {
            format!("PostgreSQL unavailable: {e}")
        } else {
            format!("Failed to prepare Postgres test database: {e}")
        };
        panic!("{msg}");
    })
}
