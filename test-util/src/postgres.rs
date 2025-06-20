#![cfg(feature = "postgres")]

use std::{error::Error as StdError, path::PathBuf};

use nix::unistd::geteuid;
use postgresql_embedded::{PostgreSQL, Settings};
use rstest::fixture;
use tempfile::TempDir;
use uuid::Uuid;

#[derive(Debug)]
pub(crate) struct EmbeddedPg {
    pub url: String,
    pub db_name: String,
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

fn generate_db_name(prefix: &str) -> String { format!("{}{}", prefix, Uuid::now_v7().simple()) }

pub(crate) fn start_embedded_postgres<F>(setup: F) -> Result<EmbeddedPg, Box<dyn std::error::Error>>
where
    F: FnOnce(&str) -> Result<(), Box<dyn std::error::Error>>,
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
    let url = pg.settings().url(&db_name);
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

    let pg = if geteuid().is_root() {
        let bin = crate::HELPER_BIN
            .clone()
            .map_err(|e| -> Box<dyn StdError> { e.into() })?;

        let lock_path = runtime_dir.join(".install_lock");
        let _lock = InstallLock::new(&lock_path)?;

        let run_status = std::process::Command::new(bin)
            .env("PG_DATA_DIR", data_dir)
            .env("PG_RUNTIME_DIR", runtime_dir)
            .env("PG_PORT", settings.port.to_string())
            .env("PG_VERSION_REQ", settings.version.to_string())
            .env("PG_SUPERUSER", &settings.username)
            .env("PG_PASSWORD", &settings.password)
            .status()
            .map_err(|e| format!("running postgres-setup-unpriv: {e}"))?;

        if !run_status.success() {
            return Err("postgres-setup-unpriv failed".into());
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

pub(crate) fn reset_postgres_db(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    use postgres::{Client, NoTls};

    let mut client = Client::connect(url, NoTls)?;
    client.batch_execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")?;
    Ok(())
}

fn drop_embedded_db(pg: &PostgreSQL, db_name: &str) {
    let admin_url = pg.settings().url("postgres");
    if let Ok(mut client) = postgres::Client::connect(&admin_url, postgres::NoTls) {
        let query = format!("DROP DATABASE IF EXISTS \"{}\"", db_name);
        if let Err(e) = client.batch_execute(&query) {
            eprintln!("error dropping database {}: {}", db_name, e);
        }
    }
}

pub struct PostgresTestDb {
    pub url: String,
    pg: Option<PostgreSQL>,
    db_name: Option<String>,
    _temp_dir: Option<TempDir>,
}

impl PostgresTestDb {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(value) = std::env::var_os("POSTGRES_TEST_URL") {
            let url = value.to_string_lossy().into_owned();
            reset_postgres_db(&url)?;
            return Ok(Self {
                url,
                pg: None,
                db_name: None,
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
            pg: Some(pg),
            db_name: Some(db_name),
            _temp_dir: Some(temp_dir),
        })
    }

    pub fn uses_embedded(&self) -> bool { self.pg.is_some() }
}

impl Drop for PostgresTestDb {
    fn drop(&mut self) {
        match (&self.pg, &self.db_name) {
            (Some(pg), Some(name)) => drop_embedded_db(pg, name),
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
