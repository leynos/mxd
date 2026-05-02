//! Shared URL, database-name, and external database helpers for `PostgreSQL` tests.

use std::{
    error::Error as StdError,
    net::{TcpStream, ToSocketAddrs},
    ops::Deref,
    time::Duration,
};

use pg_embedded_setup_unpriv::test_support::hash_directory;
use postgres::{Client, NoTls};
use url::Url;
use uuid::Uuid;

const DEFAULT_POSTGRES_PORT: u16 = 5432;

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
    /// `POSTGRES_TEST_URL` or a generated database URL could not be parsed.
    UrlParse(url::ParseError),
    /// External database creation failed.
    DbCreateFailed(String),
    /// Embedded `PostgreSQL` initialization failed.
    EmbeddedInitFailed(String),
    /// Blocking database setup task failed before returning a result.
    BlockingTaskFailed(tokio::task::JoinError),
}

impl std::fmt::Display for PostgresTestDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(e) => write!(f, "{e}"),
            Self::UrlParse(e) => write!(f, "PostgreSQL URL parse failed: {e}"),
            Self::DbCreateFailed(e) => write!(f, "PostgreSQL database creation failed: {e}"),
            Self::EmbeddedInitFailed(e) => {
                write!(f, "embedded PostgreSQL initialization failed: {e}")
            }
            Self::BlockingTaskFailed(e) => write!(f, "PostgreSQL setup task failed: {e}"),
        }
    }
}

impl StdError for PostgresTestDbError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Unavailable(e) => Some(e),
            Self::UrlParse(e) => Some(e),
            Self::BlockingTaskFailed(e) => Some(e),
            Self::DbCreateFailed(_) | Self::EmbeddedInitFailed(_) => None,
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
    if let Some((host, port)) = tcp_probe_target(url) {
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

fn tcp_probe_target(url: &Url) -> Option<(String, u16)> {
    let query_host = url
        .query_pairs()
        .find_map(|(key, value)| (key == "host").then(|| value.into_owned()))
        .filter(|host| !host.starts_with('/'));
    let host = url.host_str().map(str::to_owned).or(query_host)?;
    let query_port = url
        .query_pairs()
        .find_map(|(key, value)| (key == "port").then(|| value.parse::<u16>().ok()))
        .flatten();
    let port = query_port.or_else(|| probe_port(url))?;
    Some((host, port))
}

pub(super) fn probe_port(url: &Url) -> Option<u16> {
    url.port().or_else(|| match url.scheme() {
        "postgres" | "postgresql" => Some(DEFAULT_POSTGRES_PORT),
        _ => url.port_or_known_default(),
    })
}

pub(super) fn generate_db_name(prefix: &str) -> Result<DatabaseName, DatabaseNameError> {
    let name = format!("{prefix}{}", Uuid::now_v7().simple());
    DatabaseName::new(name)
}

pub(super) fn create_external_db_if_available(
    admin_url: &DatabaseUrl,
) -> Result<(DatabaseUrl, DatabaseName), PostgresTestDbError> {
    let parsed = Url::parse(admin_url.as_ref()).map_err(PostgresTestDbError::UrlParse)?;
    if tcp_probe_target(&parsed).is_some() && !postgres_available(&parsed) {
        return Err(PostgresTestDbError::Unavailable(PostgresUnavailable));
    }
    create_external_db(admin_url)
        .map_err(|error| PostgresTestDbError::DbCreateFailed(error.to_string()))
}

pub(super) fn postgres_test_url_from_env() -> Option<String> {
    std::env::var("POSTGRES_TEST_URL")
        .ok()
        .map(|url| url.trim().to_owned())
        .filter(|url| !url.is_empty())
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

pub(super) fn drop_external_db(admin_url: &DatabaseUrl, db_name: &DatabaseName) {
    drop_database(admin_url, db_name);
}
