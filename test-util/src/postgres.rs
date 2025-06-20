#![cfg(feature = "postgres")]

use std::{error::Error as StdError, ops::Deref, path::PathBuf};

use nix::unistd::{Uid, User, chown, geteuid};
use postgresql_embedded::{PostgreSQL, Settings};
use rstest::fixture;
use tempfile::TempDir;
use url::Url;
use uuid::Uuid;

/// Fallback UID for the `nobody` user if lookup fails.
const NOBODY_UID: u32 = 65_534;

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

#[derive(Debug)]
pub(crate) struct EmbeddedPg {
    pub url: DatabaseUrl,
    pub db_name: DatabaseName,
    pub pg: PostgreSQL,
    pub temp_dir: TempDir,
    pub _runtime_temp: Option<TempDir>,
}

struct InstallLock {
    file: Option<std::fs::File>,
    path: PathBuf,
}

impl InstallLock {
    fn new(path: &std::path::Path) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(path)?;
        if let Err(e) = fs2::FileExt::lock_exclusive(&file) {
            let _ = std::fs::remove_file(path);
            return Err(e);
        }
        Ok(Self {
            file: Some(file),
            path: path.to_path_buf(),
        })
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            let _ = fs2::FileExt::unlock(&file);
            drop(file);
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

fn generate_db_name(prefix: &str) -> DatabaseName {
    let name = format!("{}{}", prefix, Uuid::now_v7().simple());
    DatabaseName::new(name).expect("generated database name")
}

pub(crate) fn start_embedded_postgres<F>(setup: F) -> Result<EmbeddedPg, Box<dyn std::error::Error>>
where
    F: FnOnce(&DatabaseUrl) -> Result<(), Box<dyn std::error::Error>>,
{
    let fut = async { init_embedded_postgres().await };
    let embedded = match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.block_on(fut)?,
        Err(_) => tokio::runtime::Runtime::new()?.block_on(fut)?,
    };
    setup(&embedded.url)?;
    Ok(embedded)
}

async fn init_embedded_postgres() -> Result<EmbeddedPg, Box<dyn std::error::Error>> {
    let mut settings = Settings::default();
    let tmp = tempfile::Builder::new().prefix("mxd-pg").tempdir()?;
    let data_dir = tmp.path().to_path_buf();
    settings.data_dir = data_dir.clone();

    let (runtime_dir, runtime_temp) = prepare_runtime_dir()?;
    let mut pg = prepare_postgres(settings.clone(), &runtime_dir, &data_dir).await?;

    if let Err(e) = pg.start().await {
        let _ = pg.stop().await;
        return Err(format!("starting embedded PostgreSQL: {e}").into());
    }
    let db_name = generate_db_name("test_");
    if let Err(e) = pg.create_database(&db_name).await {
        let _ = pg.stop().await;
        return Err(format!("creating test database {db_name}: {e}").into());
    }
    let url = DatabaseUrl::parse(&pg.settings().url(&db_name))?;
    Ok(EmbeddedPg {
        url,
        db_name,
        pg,
        temp_dir: tmp,
        _runtime_temp: runtime_temp,
    })
}

fn prepare_runtime_dir() -> Result<(PathBuf, Option<TempDir>), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        Ok((PathBuf::from("/usr/libexec/theseus"), None))
    }
    #[cfg(not(unix))]
    {
        let runtime_temp = tempfile::Builder::new().prefix("mxd-runtime").tempdir()?;
        let runtime_dir = runtime_temp.path().to_path_buf();
        Ok((runtime_dir, Some(runtime_temp)))
    }
}

async fn prepare_postgres(
    mut settings: Settings,
    runtime_dir: &std::path::Path,
    data_dir: &std::path::Path,
) -> Result<PostgreSQL, Box<dyn std::error::Error>> {
    settings.installation_dir = runtime_dir.to_path_buf();

    // Always ensure the runtime directory exists. The helper binary expects
    // it to be present even when running without privileges.
    std::fs::create_dir_all(runtime_dir)?;

    #[cfg(unix)]
    if geteuid().is_root() {
        let uid = User::from_name("nobody")
            .ok()
            .flatten()
            .map(|u| u.uid)
            .unwrap_or_else(|| Uid::from_raw(NOBODY_UID));
        if let Err(e) = chown(runtime_dir, Some(uid), None) {
            eprintln!(
                "warning: failed to chown {} to nobody: {}",
                runtime_dir.display(),
                e
            );
        }
    }

    let pg = if geteuid().is_root() {
        let bin = crate::HELPER_BIN
            .clone()
            .map_err(|e| -> Box<dyn StdError> { e.into() })?;

        let lock_path = runtime_dir.join(".install_lock");
        let _lock = InstallLock::new(&lock_path)?;

        let status = std::process::Command::new(bin)
            .env("PG_DATA_DIR", data_dir)
            .env("PG_RUNTIME_DIR", runtime_dir)
            .env("PG_PORT", settings.port.to_string())
            .env("PG_VERSION_REQ", settings.version.to_string())
            .env("PG_SUPERUSER", &settings.username)
            .env("PG_PASSWORD", &settings.password)
            .status()
            .map_err(|e| format!("running postgres-setup-unpriv: {e}"))?;

        if !status.success() {
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".into());
            return Err(format!("postgres-setup-unpriv failed with status {code}").into());
        }

        PostgreSQL::new(settings)
    } else {
        let mut pg = PostgreSQL::new(settings);
        if let Err(e) = pg.setup().await {
            let _ = pg.stop().await;
            return Err(format!("preparing embedded PostgreSQL: {e}").into());
        }
        pg
    };
    Ok(pg)
}

pub(crate) fn reset_postgres_db(url: &DatabaseUrl) -> Result<(), Box<dyn std::error::Error>> {
    use postgres::{Client, NoTls};

    let mut client = Client::connect(url.as_ref(), NoTls)?;
    client.batch_execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")?;
    Ok(())
}

fn drop_embedded_db(pg: &PostgreSQL, db_name: &DatabaseName) {
    let admin_url = pg.settings().url("postgres");
    if let Ok(mut client) = postgres::Client::connect(&admin_url, postgres::NoTls) {
        let query = format!("DROP DATABASE IF EXISTS \"{}\"", db_name);
        if let Err(e) = client.batch_execute(&query) {
            eprintln!("error dropping database {}: {}", db_name, e);
        }
    }
}

fn create_external_db(
    base_url: &DatabaseUrl,
) -> Result<(DatabaseUrl, DatabaseName), Box<dyn StdError>> {
    use postgres::{Client, NoTls};
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
    if let Ok(mut client) = postgres::Client::connect(admin_url.as_ref(), postgres::NoTls) {
        let query = format!("DROP DATABASE IF EXISTS \"{}\"", db_name);
        if let Err(e) = client.batch_execute(&query) {
            eprintln!("error dropping database {}: {}", db_name, e);
        }
    }
}

pub struct PostgresTestDb {
    pub url: DatabaseUrl,
    admin_url: Option<DatabaseUrl>,
    pg: Option<PostgreSQL>,
    db_name: Option<DatabaseName>,
    _temp_dir: Option<TempDir>,
}

impl PostgresTestDb {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(value) = std::env::var_os("POSTGRES_TEST_URL") {
            let admin_url = DatabaseUrl::parse(&value.to_string_lossy())?;
            let (url, db_name) = create_external_db(&admin_url)?;
            return Ok(Self {
                url,
                admin_url: Some(admin_url),
                pg: None,
                db_name: Some(db_name),
                _temp_dir: None,
            });
        }

        let EmbeddedPg {
            url,
            pg,
            temp_dir,
            db_name,
            ..
        } = start_embedded_postgres(|url| reset_postgres_db(url))?;
        Ok(Self {
            url,
            admin_url: None,
            pg: Some(pg),
            db_name: Some(db_name),
            _temp_dir: Some(temp_dir),
        })
    }

    pub fn uses_embedded(&self) -> bool { self.pg.is_some() }
}

impl Drop for PostgresTestDb {
    fn drop(&mut self) {
        match (&self.pg, &self.db_name, &self.admin_url) {
            (Some(pg), Some(name), _) => drop_embedded_db(pg, name),
            (None, Some(name), Some(admin)) => drop_external_db(admin, name),
            _ => {
                let _ = reset_postgres_db(&self.url);
            }
        }
        if let Some(pg) = self.pg.take() {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(pg.stop());
        }
    }
}

#[fixture]
pub fn postgres_db() -> PostgresTestDb {
    PostgresTestDb::new().expect("Failed to prepare Postgres test database")
}
