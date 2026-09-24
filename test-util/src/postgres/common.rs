//! Shared URL, database-name, and cleanup helpers for embedded `PostgreSQL` tests.

use std::{error::Error as StdError, ops::Deref};

use pg_embedded_setup_unpriv::test_support::hash_directory;
use postgres::{Client, NoTls};
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

/// Error type for [`PostgresTestDb::new`].
///
/// There is no "unavailable" outcome: every `PostgreSQL` test runs against an
/// embedded cluster, so a cluster that cannot be bootstrapped is a failure to
/// report, never a reason to skip.
#[derive(Debug)]
pub enum PostgresTestDbError {
    /// The embedded `PostgreSQL` cluster could not be bootstrapped or started.
    EmbeddedBootstrapFailed(String),
    /// The embedded cluster started but a test database could not be prepared.
    EmbeddedInitFailed(String),
}

impl std::fmt::Display for PostgresTestDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmbeddedBootstrapFailed(e) => {
                write!(f, "embedded PostgreSQL bootstrap failed: {e}")
            }
            Self::EmbeddedInitFailed(e) => {
                write!(f, "embedded PostgreSQL initialization failed: {e}")
            }
        }
    }
}

impl StdError for PostgresTestDbError {}

pub(super) fn generate_db_name(prefix: &str) -> Result<DatabaseName, DatabaseNameError> {
    let name = format!("{prefix}{}", Uuid::now_v7().simple());
    DatabaseName::new(name)
}

/// Generates a stable template name based on migration content hash.
/// Template name changes when migrations change, forcing template recreation.
pub(super) fn migration_template_name() -> Result<DatabaseName, Box<dyn StdError + Send + Sync>> {
    let hash = hash_directory("migrations")?;
    // Use first 8 chars of hash for a reasonably unique but readable name.
    // The hash is always hexadecimal ASCII, so char boundary indexing is safe.
    let prefix = hash.get(..8).unwrap_or(&hash);
    DatabaseName::new(format!("template_{prefix}")).map_err(|e| Box::new(e) as _)
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
pub(super) fn drop_database(admin_url: &DatabaseUrl, db_name: &DatabaseName) {
    let admin_url_owned = admin_url.clone();
    let db_name_owned = db_name.clone();
    // Spawn a fresh OS thread so that the synchronous `postgres` crate does not
    // attempt to block_on inside an already-active Tokio runtime, which would
    // panic with "Cannot start a runtime from within a runtime."
    let result = std::thread::spawn(move || {
        let mut client = match Client::connect(admin_url_owned.as_ref(), NoTls) {
            Ok(client) => client,
            Err(err) => {
                let redacted_url = redacted_database_url(&admin_url_owned);
                eprintln!("error connecting to admin database {redacted_url}: {err}");
                return;
            }
        };
        if let Err(e) = client.execute(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1 AND pid <> \
             pg_backend_pid()",
            &[&db_name_owned.as_ref()],
        ) {
            eprintln!("error terminating active connections for database {db_name_owned}: {e}");
        }
        let query = format!("DROP DATABASE IF EXISTS \"{db_name_owned}\"");
        if let Err(e) = client.batch_execute(&query) {
            eprintln!("error dropping database {db_name_owned}: {e}");
        }
    })
    .join();
    if let Err(e) = result {
        eprintln!("drop_database cleanup thread panicked: {e:?}");
    }
}

fn redacted_database_url(url: &DatabaseUrl) -> String {
    let Ok(mut parsed_url) = Url::parse(url.as_ref()) else {
        return "<invalid database URL>".to_owned();
    };
    if !parsed_url.username().is_empty() && parsed_url.set_username("<redacted>").is_err() {
        return "<invalid database URL>".to_owned();
    }
    if parsed_url.password().is_some() && parsed_url.set_password(Some("<redacted>")).is_err() {
        return "<invalid database URL>".to_owned();
    }
    parsed_url.to_string()
}
