#![cfg(feature = "postgres")]

//! Helpers for PostgreSQL-backed integration tests.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseUrl(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseName(String);

impl DatabaseUrl {
    pub fn parse(url: &str) -> Result<Self, url::ParseError> {
        Url::parse(url)?;
        Ok(Self(url.to_string()))
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

#[derive(Debug)]
pub struct PostgresUnavailable;

impl std::fmt::Display for PostgresUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PostgreSQL server unreachable")
    }
}

impl std::error::Error for PostgresUnavailable {}

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

fn generate_db_name(prefix: &str) -> DatabaseName {
    let name = format!("{}{}", prefix, Uuid::now_v7().simple());
    DatabaseName::new(name).expect("generated database name")
}

pub(crate) fn start_embedded_postgres<F>(
    setup: F,
) -> Result<EmbeddedPg, Box<dyn StdError + Send + Sync>>
where
    F: FnOnce(&DatabaseUrl) -> Result<(), Box<dyn StdError + Send + Sync>>,
{
    let cluster =
        TestCluster::new().map_err(|e| format!("bootstrapping embedded PostgreSQL: {e}"))?;
    let connection = cluster.connection();
    let admin_url = DatabaseUrl::parse(&connection.database_url("postgres"))?;
    let (url, db_name) = create_external_db(&admin_url)?;
    setup(&url)?;
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

fn drop_database(admin_url: &DatabaseUrl, db_name: &DatabaseName) {
    if let Ok(mut client) = Client::connect(admin_url.as_ref(), NoTls) {
        let query = format!("DROP DATABASE IF EXISTS \"{}\"", db_name);
        if let Err(e) = client.batch_execute(&query) {
            eprintln!("error dropping database {}: {}", db_name, e);
        }
    }
}

fn create_external_db(
    base_url: &DatabaseUrl,
) -> Result<(DatabaseUrl, DatabaseName), Box<dyn StdError + Send + Sync>> {
    let mut url = Url::parse(base_url.as_ref())?;
    let db_name = generate_db_name("test_");
    let admin_url = url.to_string();
    let mut client = Client::connect(&admin_url, NoTls)?;
    let query = format!("CREATE DATABASE \"{}\"", db_name);
    client.batch_execute(&query)?;
    url.set_path(&db_name);
    Ok((DatabaseUrl::parse(url.as_str())?, db_name))
}

fn drop_external_db(admin_url: &DatabaseUrl, db_name: &DatabaseName) {
    drop_database(admin_url, db_name);
}

pub struct PostgresTestDb {
    pub url: DatabaseUrl,
    admin_url: Option<DatabaseUrl>,
    embedded: Option<EmbeddedPg>,
    db_name: Option<DatabaseName>,
}

impl PostgresTestDb {
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

        let embedded = match start_embedded_postgres(reset_postgres_db) {
            Ok(pg) => pg,
            Err(_) => return Err(Box::new(PostgresUnavailable)),
        };
        let url = embedded.url.clone();
        let db_name = embedded.db_name.clone();
        Ok(Self {
            url,
            admin_url: None,
            embedded: Some(embedded),
            db_name: Some(db_name),
        })
    }

    pub fn uses_embedded(&self) -> bool { self.embedded.is_some() }
}

impl Drop for PostgresTestDb {
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

#[fixture]
pub fn postgres_db() -> PostgresTestDb {
    PostgresTestDb::new().expect("Failed to prepare Postgres test database")
}
