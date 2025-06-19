//! Utilities for integration tests.
//!
//! The `test-util` crate provides helpers to spin up temporary servers and,
//! when the `postgres` feature is enabled, manage embedded PostgreSQL
//! instances. It is used by integration tests in the main crate.
#[cfg(feature = "postgres")]
use std::error::Error as StdError;
#[cfg(feature = "postgres")]
use std::path::{Path, PathBuf};
use std::{
    io::{BufRead, BufReader},
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(feature = "postgres")]
use nix::unistd::geteuid;
#[cfg(feature = "postgres")]
use once_cell::sync::Lazy;
#[cfg(feature = "postgres")]
use postgresql_embedded::PostgreSQL;
#[cfg(feature = "postgres")]
use postgresql_embedded::Settings;
#[cfg(feature = "postgres")]
use rstest::fixture;
use tempfile::TempDir;
#[cfg(feature = "postgres")]
use tracing::warn;
#[cfg(feature = "postgres")]
use uuid::Uuid;

#[cfg(feature = "postgres")]
static HELPER_BIN: Lazy<Result<PathBuf, String>> = Lazy::new(|| {
    let manifest = std::env::var("POSTGRES_SETUP_UNPRIV_MANIFEST").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../postgres_setup_unpriv/Cargo.toml")
            .to_string_lossy()
            .into_owned()
    });

    let bin = Path::new(&manifest)
        .parent()
        .expect("manifest path has parent")
        .join("target/debug/postgres-setup-unpriv");

    // Rebuild the helper once per test session. Cargo is fast when nothing
    // changed, so always delegate the up-to-date check to it.
    let status = std::process::Command::new("cargo")
        .args([
            "build",
            "--bin",
            "postgres-setup-unpriv",
            "--manifest-path",
            &manifest,
            "--quiet",
        ])
        .status()
        .map_err(|e| format!("building postgres-setup-unpriv: {e}"))?;
    if !status.success() {
        return Err("building postgres-setup-unpriv failed".into());
    }

    Ok(bin)
});

#[cfg(all(feature = "sqlite", feature = "postgres"))]
compile_error!("Choose either sqlite or postgres, not both");

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("Either feature 'sqlite' or 'postgres' must be enabled");

#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::unistd::Pid;

/// Helper struct to start the mxd server for integration tests.
///
/// The server process is terminated when this struct is dropped.
pub struct TestServer {
    child: Child,
    port: u16,
    db_url: String,
    #[cfg(feature = "postgres")]
    pg: Option<PostgreSQL>,
    /// Keep the temporary directory alive for the lifetime of the server to
    /// prevent early cleanup while the database is running.
    temp_dir: Option<TempDir>,
}

#[cfg(feature = "postgres")]
#[derive(Debug)]
struct EmbeddedPg {
    url: String,
    db_name: String,
    pg: PostgreSQL,
    temp_dir: TempDir,
    _runtime_temp: Option<TempDir>,
}

#[cfg(feature = "postgres")]
/// Resources required to run tests with a PostgreSQL database.
///
/// This wrapper combines the connection URL, the optional embedded PostgreSQL
/// instance, and the optional temporary directory used by the embedded server.
struct DbResources {
    url: String,
    pg: Option<PostgreSQL>,
    temp_dir: Option<TempDir>,
}

#[cfg(feature = "postgres")]
fn generate_db_name(prefix: &str) -> String { format!("{}{}", prefix, Uuid::now_v7().simple()) }

#[cfg(feature = "postgres")]
/// Start an embedded PostgreSQL instance for tests.
///
/// Returns an [`EmbeddedPg`] containing the connection URL, the
/// running [`PostgreSQL`] handle, and the [`TempDir`] of the data
/// directory. The temporary directory must be kept alive for as long
/// as the server is running, otherwise the data directory would be
/// removed prematurely.
fn start_embedded_postgres<F>(setup: F) -> Result<EmbeddedPg, Box<dyn std::error::Error>>
where
    F: FnOnce(&str) -> Result<(), Box<dyn std::error::Error>>,
{
    let fut = async {
        let mut settings = Settings::default();
        let tmp = tempfile::Builder::new().prefix("mxd-pg").tempdir()?;
        let data_dir = tmp.path().to_path_buf();
        settings.data_dir = data_dir.clone();

        #[cfg(unix)]
        let runtime_temp = None;
        #[cfg(not(unix))]
        let runtime_temp = Some(tempfile::Builder::new().prefix("mxd-runtime").tempdir()?);

        #[cfg(unix)]
        let runtime_dir = std::path::PathBuf::from("/usr/libexec/theseus");
        #[cfg(not(unix))]
        let runtime_dir = runtime_temp.as_ref().unwrap().path().to_path_buf();

        settings.installation_dir = runtime_dir.clone();

        let mut pg = if geteuid().is_root() {
            let bin = HELPER_BIN
                .clone()
                .map_err(|e| -> Box<dyn StdError> { e.into() })?;

            // Lock the runtime directory to avoid concurrent modifications.
            let lock_path = runtime_dir.join(".install_lock");
            let lock_file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&lock_path)?;
            fs2::FileExt::lock_exclusive(&lock_file)?;

            let run_status = std::process::Command::new(bin)
                .env("PG_DATA_DIR", &data_dir)
                .env("PG_RUNTIME_DIR", &runtime_dir)
                .env("PG_PORT", settings.port.to_string())
                .env("PG_VERSION_REQ", settings.version.to_string())
                .env("PG_SUPERUSER", &settings.username)
                .env("PG_PASSWORD", &settings.password)
                .status()
                .map_err(|e| format!("running postgres-setup-unpriv: {e}"))?;

            // Release the lock immediately after setup completes.
            fs2::FileExt::unlock(&lock_file)?;

            if !run_status.success() {
                return Err("postgres-setup-unpriv failed".into());
            }

            PostgreSQL::new(settings.clone())
        } else {
            let mut pg = PostgreSQL::new(settings.clone());
            if let Err(e) = pg.setup().await {
                let _ = pg.stop().await;
                return Err(format!("preparing embedded PostgreSQL: {e}").into());
            }
            pg
        };

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
        Ok::<_, Box<dyn StdError>>(EmbeddedPg {
            url,
            db_name,
            pg,
            temp_dir: tmp,
            _runtime_temp: runtime_temp,
        })
    };
    let embedded = match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.block_on(fut)?,
        Err(_) => tokio::runtime::Runtime::new()?.block_on(fut)?,
    };
    setup(&embedded.url)?;
    Ok(embedded)
}

/// Resets a PostgreSQL database by dropping and recreating the public schema.
///
/// This function ensures a completely clean database state by removing all tables,
/// functions, types, and other objects in the `public` schema, then creating a
/// fresh empty schema.
///
/// ## Schema Reset Process
///
/// 1. Executes `DROP SCHEMA public CASCADE` to remove all objects
/// 2. Executes `CREATE SCHEMA public` to create a fresh empty schema
///
/// The `CASCADE` option ensures that all dependent objects are removed automatically,
/// providing a thorough cleanup regardless of the database's previous state.
///
/// ## Parameters
///
/// * `url` - PostgreSQL connection string for the database to reset
///
/// ## Returns
///
/// Returns `Ok(())` on successful schema reset, or an error if the database
/// connection or SQL execution fails.
///
/// ## Errors
///
/// This function will return an error if:
/// - The database connection cannot be established
/// - The SQL commands fail to execute (e.g., insufficient permissions)
///
/// ## Examples
///
/// ```rust
/// let db_url = "postgresql://localhost/testdb";
/// reset_postgres_db(db_url)?;
/// // Database now has a clean public schema
/// ```
#[cfg(feature = "postgres")]
fn reset_postgres_db(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    use postgres::{Client, NoTls};

    let mut client = Client::connect(url, NoTls)?;
    client.batch_execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")?;
    Ok(())
}

#[cfg(feature = "postgres")]
fn retry_postgres<F>(mut op: F) -> Result<(), postgres::Error>
where
    F: FnMut() -> Result<(), postgres::Error>,
{
    use std::{thread, time::Duration};

    let mut delay = Duration::from_millis(50);
    for attempt in 0..5 {
        match op() {
            Ok(()) => return Ok(()),
            Err(_e) if attempt < 4 => {
                thread::sleep(delay);
                delay = std::cmp::min(delay * 2, Duration::from_secs(1));
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!();
}

#[cfg(feature = "postgres")]
fn create_db(admin_url: &str, db_name: &str) -> Result<(), postgres::Error> {
    retry_postgres(|| {
        let mut client = postgres::Client::connect(admin_url, postgres::NoTls)?;
        client.batch_execute(&format!("CREATE DATABASE \"{}\"", db_name))
    })
}

#[cfg(feature = "postgres")]
fn drop_db(admin_url: &str, db_name: &str) -> Result<(), postgres::Error> {
    retry_postgres(|| {
        let mut client = postgres::Client::connect(admin_url, postgres::NoTls)?;
        client.batch_execute(&format!("DROP DATABASE IF EXISTS \"{}\"", db_name))
    })
}

#[cfg(feature = "postgres")]
fn drop_embedded_db(pg: &PostgreSQL, db_name: &str) -> Result<(), postgres::Error> {
    let admin_url = pg.settings().url("postgres");
    drop_db(&admin_url, db_name)
}

#[cfg(feature = "postgres")]
fn create_external_db(
    admin_url: &str,
    db_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use url::Url;

    create_db(admin_url, db_name)?;
    let mut url = Url::parse(admin_url)?;
    url.set_path(&format!("/{}", db_name));
    Ok(url.to_string())
}

#[cfg(feature = "postgres")]
fn drop_external_db(admin_url: &str, db_name: &str) -> Result<(), postgres::Error> {
    drop_db(admin_url, db_name)
}

/// RAII-style PostgreSQL test database fixture that ensures clean schema state.
///
/// This struct manages the lifecycle of a PostgreSQL database for testing, automatically
/// handling schema reset both on creation and destruction. It follows the RAII pattern
/// to guarantee that each test gets a pristine database schema regardless of how the
/// previous test terminated.
///
/// The database can be either:
/// - A dedicated database on an external PostgreSQL server specified via `POSTGRES_TEST_URL`
/// - An embedded PostgreSQL server managed by this fixture
///
/// ## Schema Management
///
/// The fixture ensures database cleanliness by:
/// 1. Dropping and recreating the `public` schema on initialization
/// 2. Dropping and recreating the `public` schema on destruction (via `Drop` trait)
///
/// This guarantees that each test starts with a completely clean schema, even if previous
/// tests crashed or left the database in an inconsistent state.
///
/// ## Usage with External Database
///
/// Set the `POSTGRES_TEST_URL` environment variable to point to an existing
/// PostgreSQL server. A new database will be created for each test fixture:
/// ```bash
/// export POSTGRES_TEST_URL="postgresql://user:pass@localhost/testdb"
/// ```
///
/// ## Usage with Embedded Database
///
/// If `POSTGRES_TEST_URL` is not set, an embedded PostgreSQL server will be started
/// automatically and stopped when the fixture is dropped.
///
/// ## Examples
///
/// ```rust
/// use rstest::rstest;
/// use test_util::postgres_db;
///
/// #[rstest]
/// fn test_with_clean_database(postgres_db: PostgresTestDb) {
///     // Test runs with a clean PostgreSQL schema
///     let db_url = &postgres_db.url;
///     // ... test implementation
/// }
/// ```
#[cfg(feature = "postgres")]
pub struct PostgresTestDb {
    /// PostgreSQL connection URL for the test database.
    ///
    /// This URL can be used to establish connections to the test database.
    /// The database is guaranteed to have a clean `public` schema.
    pub url: String,
    /// Optional embedded PostgreSQL instance.
    ///
    /// This is `Some` when using an embedded server, `None` when using an external
    /// database specified via `POSTGRES_TEST_URL`.
    pg: Option<PostgreSQL>,
    /// Name of the database created for this instance.
    db_name: Option<String>,
    /// Base connection URL used to create and drop the database when using an
    /// external PostgreSQL server.
    admin_url: Option<String>,
    /// Hold the data directory alive until after the embedded server stops.
    _temp_dir: Option<TempDir>,
}

#[cfg(feature = "postgres")]
impl PostgresTestDb {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(value) = std::env::var_os("POSTGRES_TEST_URL") {
            let admin_url = value.to_string_lossy().into_owned();
            let db_name = generate_db_name("test_");
            let url = create_external_db(&admin_url, &db_name)?;
            reset_postgres_db(&url)?;
            return Ok(Self {
                url,
                pg: None,
                db_name: Some(db_name),
                admin_url: Some(admin_url),
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
            admin_url: None,
            _temp_dir: Some(temp_dir),
        })
    }

    pub fn uses_embedded(&self) -> bool { self.pg.is_some() }
}

#[cfg(feature = "postgres")]
impl Drop for PostgresTestDb {
    fn drop(&mut self) {
        if let (Some(name), Some(admin)) = (self.db_name.as_deref(), self.admin_url.as_deref()) {
            if let Err(e) = drop_external_db(admin, name) {
                warn!(%name, error = %e, "failed to drop external test database");
            }
        } else if let (Some(pg), Some(name)) = (self.pg.as_ref(), self.db_name.as_deref()) {
            if let Err(e) = drop_embedded_db(pg, name) {
                warn!(%name, error = %e, "failed to drop embedded test database");
            }
        }
        if let Some(pg) = self.pg.take() {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(pg.stop());
        }
    }
}

/// Test fixture that provides a clean PostgreSQL database for each test.
///
/// This fixture creates a `PostgresTestDb` instance that manages database lifecycle
/// and schema cleanliness automatically. It can be injected into any test function
/// that requires PostgreSQL access.
///
/// ## Behavior
///
/// - **External Database**: If `POSTGRES_TEST_URL` is set, creates a temporary database on that
///   server
/// - **Embedded Database**: Otherwise, starts an embedded PostgreSQL server
/// - **Schema Reset**: Drops and recreates the `public` schema before each test
/// - **Cleanup**: Drops the database and stops the embedded server after each test
///
/// ## Environment Variable
///
/// Set `POSTGRES_TEST_URL` to point to an existing PostgreSQL server. The
/// fixture will create and later drop a temporary database:
/// ```bash
/// export POSTGRES_TEST_URL="postgresql://localhost/testdb"
/// ```
///
/// ## Usage
///
/// ```rust
/// use rstest::rstest;
/// use test_util::{PostgresTestDb, postgres_db};
///
/// #[rstest]
/// fn my_test(postgres_db: PostgresTestDb) {
///     // Test has access to clean PostgreSQL database
///     let connection_url = &postgres_db.url;
///     // ... test implementation
/// }
/// ```
///
/// ## Panics
///
/// Panics if database setup fails, which indicates a fundamental testing environment issue.
#[cfg(feature = "postgres")]
#[fixture]
pub fn postgres_db() -> PostgresTestDb {
    PostgresTestDb::new().expect("Failed to prepare Postgres test database")
}

///
/// Returns the SQLite database URL as a string on success, or an error if setup fails.
///
/// # Examples
///
/// ```
/// use tempfile::TempDir;
///
/// let temp_dir = TempDir::new().unwrap();
/// let db_url = setup_sqlite(&temp_dir, |path| {
///     // Custom setup logic, e.g., run migrations
///     Ok(())
/// })
/// .unwrap();
/// assert!(db_url.ends_with("mxd.db"));
/// ```
#[cfg(feature = "sqlite")]
fn setup_sqlite<F>(temp: &TempDir, setup: F) -> Result<String, Box<dyn std::error::Error>>
where
    F: FnOnce(&str) -> Result<(), Box<dyn std::error::Error>>,
{
    let path = temp.path().join("mxd.db");
    setup(path.to_str().expect("db path utf8"))?;
    Ok(path.to_str().unwrap().to_owned())
}

fn wait_for_server(child: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(out) = &mut child.stdout {
        let mut reader = BufReader::new(out);
        let mut line = String::new();
        let timeout = Duration::from_secs(10);
        let start = Instant::now();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                return Err("server exited before signaling readiness".into());
            }
            if line.contains("listening on") {
                break;
            }
            if start.elapsed() > timeout {
                return Err("timeout waiting for server to signal readiness".into());
            }
        }
        Ok(())
    } else {
        Err("missing stdout from server".into())
    }
}

fn build_server_command(manifest_path: &str, port: u16, db_url: &str) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    #[cfg(feature = "postgres")]
    cmd.args(["--no-default-features", "--features", "postgres"]);
    #[cfg(feature = "sqlite")]
    cmd.args(["--features", "sqlite"]);
    cmd.args([
        "--bin",
        "mxd",
        "--manifest-path",
        manifest_path,
        "--quiet",
        "--",
        "--bind",
        &format!("127.0.0.1:{port}"),
        "--database",
        db_url,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit());
    cmd
}

impl TestServer {
    /// Start the server using the given Cargo manifest path.
    pub fn start(manifest_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_setup(manifest_path, |_| Ok(()))
    }

    /// Starts the test server after running a setup function on a temporary database.
    ///
    /// The setup function is called with the database URL before the server is launched, allowing
    /// initialisation or seeding of the database for integration tests. The server is started on a
    /// random available port.
    ///
    /// # Parameters
    /// - `manifest_path`: Path to the Cargo manifest for the server binary.
    /// - `setup`: Function to run database setup logic, receiving the database URL.
    ///
    /// # Returns
    /// Returns a `TestServer` instance managing the server process and test database.
    ///
    /// # Errors
    /// Returns an error if temporary directory creation, database setup, server startup, or
    /// protocol handshake fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let server = TestServer::start_with_setup("path/to/Cargo.toml", |db_url| {
    ///     // Custom setup logic here
    ///     Ok(())
    /// })?;
    pub fn start_with_setup<F>(
        manifest_path: &str,
        setup: F,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: FnOnce(&str) -> Result<(), Box<dyn std::error::Error>>,
    {
        #[cfg(feature = "sqlite")]
        {
            let temp = TempDir::new()?;
            let db_url = setup_sqlite(&temp, setup)?;
            return Self::launch(manifest_path, db_url, Some(temp));
        }

        #[cfg(feature = "postgres")]
        {
            let resources = if let Some(value) = std::env::var_os("POSTGRES_TEST_URL") {
                let url = value.to_string_lossy().into_owned();
                reset_postgres_db(&url)?;
                DbResources {
                    url,
                    pg: None,
                    temp_dir: None,
                }
            } else {
                let EmbeddedPg {
                    url, pg, temp_dir, ..
                } = start_embedded_postgres(|url| reset_postgres_db(url))?;
                DbResources {
                    url,
                    pg: Some(pg),
                    temp_dir: Some(temp_dir),
                }
            };
            setup(&resources.url)?;
            return Self::launch(
                manifest_path,
                resources.url,
                resources.temp_dir,
                resources.pg,
            );
        }
    }

    #[cfg(feature = "sqlite")]
    /// Launches the `mxd` server on a random available port with the specified database URL for
    /// integration testing.
    ///
    /// Binds a TCP listener to obtain a free port, starts the server process with the given
    /// manifest path and database URL, waits for the server to become ready, and returns a
    /// `TestServer` instance managing the process and optional temporary directory.
    ///
    /// # Returns
    ///
    /// A `TestServer` instance managing the running server and associated resources.
    ///
    /// # Errors
    ///
    /// Returns an error if binding the port, spawning the server process, or waiting for server
    /// readiness fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let temp_dir = tempfile::TempDir::new().unwrap();
    /// let server = TestServer::launch(
    ///     "path/to/Cargo.toml",
    ///     "sqlite://test.db".to_string(),
    ///     Some(temp_dir),
    /// )
    /// .unwrap();
    /// assert!(server.port() > 0);
    /// ```
    fn launch(
        manifest_path: &str,
        db_url: String,
        temp_dir: Option<TempDir>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = TcpListener::bind("127.0.0.1:0")?;
        let port = socket.local_addr()?.port();
        drop(socket);

        let mut child = build_server_command(manifest_path, port, &db_url).spawn()?;

        wait_for_server(&mut child)?;

        Ok(Self {
            child,
            port,
            db_url,
            temp_dir,
        })
    }

    #[cfg(feature = "postgres")]
    /// Launches a test instance of the `mxd` server using the specified database and configuration.
    ///
    /// Binds the server to a random available local port, starts the server process with the
    /// provided manifest path and database URL, and waits for the server to become ready.
    /// Optionally manages a temporary directory for SQLite and an embedded PostgreSQL instance if
    /// used.
    ///
    /// # Returns
    ///
    /// A `TestServer` instance managing the server process and associated resources.
    ///
    /// # Errors
    ///
    /// Returns an error if binding the port, spawning the server process, or waiting for server
    /// readiness fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let server = TestServer::launch("Cargo.toml", db_url, None, None)?;
    /// assert!(server.port() > 0);
    /// ```
    fn launch(
        manifest_path: &str,
        db_url: String,
        temp_dir: Option<TempDir>,
        pg: Option<PostgreSQL>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = TcpListener::bind("127.0.0.1:0")?;
        let port = socket.local_addr()?.port();
        drop(socket);

        let mut child = build_server_command(manifest_path, port, &db_url).spawn()?;

        wait_for_server(&mut child)?;

        Ok(Self {
            child,
            port,
            db_url,
            temp_dir,
            pg,
        })
    }

    /// Return the port the server is bound to.
    pub fn port(&self) -> u16 { self.port }

    /// Return the database connection URL used by the server.
    ///
    /// The URL is stored as a `String` validated at construction time.
    /// Tests use SQLite by default, with optional PostgreSQL support via the
    /// Returns the database connection URL used by the test server.
    pub fn db_url(&self) -> &str {
        // `db_url` was validated when the server was created, so borrowing is
        // safe and avoids repeated validation.
        self.db_url.as_str()
    }

    /// Return the temporary directory used by the test server, if any.
    ///
    /// Keeping this handle alive prevents the directory from being deleted
    /// while the server is running.
    pub fn temp_dir(&self) -> Option<&TempDir> { self.temp_dir.as_ref() }

    #[cfg(feature = "postgres")]
    /// Returns true if the server is using an embedded PostgreSQL instance.
    ///
    /// This indicates that the test server started and manages its own embedded PostgreSQL
    /// database. Returns false if an external PostgreSQL instance is used instead.
    pub fn uses_embedded_postgres(&self) -> bool { self.pg.is_some() }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = kill(Pid::from_raw(self.child.id() as i32), Signal::SIGTERM);
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        #[cfg(feature = "postgres")]
        if let Some(pg) = &mut self.pg {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(pg.stop());
        }
    }
}

use std::{
    io::{Read, Write},
    net::TcpStream,
};

/// Send a valid protocol handshake and read the server reply.
pub fn handshake(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"TRTP");
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&buf)?;
    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(
        &reply[0..4],
        b"TRTP",
        "protocol mismatch in handshake reply"
    );
    let code = u32::from_be_bytes(reply[4..8].try_into().unwrap());
    assert_eq!(code, 0, "handshake returned error code {}", code);
    Ok(())
}

use chrono::{DateTime, Utc};
use diesel_async::{AsyncConnection, RunQueryDsl};
use futures_util::future::BoxFuture;
use mxd::{
    db::{DbConnection, add_file_acl, apply_migrations, create_category, create_file, create_user},
    models::{NewArticle, NewCategory, NewFileAcl, NewFileEntry, NewUser},
    users::hash_password,
};

/// Executes an asynchronous database setup function within a temporary Tokio runtime.
///
/// Establishes a database connection, runs migrations, and invokes the provided async closure with
/// the connection. Suitable for preparing test databases synchronously from non-async contexts.
///
/// # Parameters
/// - `db`: Database connection string.
///
/// # Returns
/// Returns `Ok(())` if the setup function completes successfully; otherwise, returns an error.
pub fn with_db<F>(db: &str, f: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: for<'c> FnOnce(
        &'c mut DbConnection,
    ) -> BoxFuture<'c, Result<(), Box<dyn std::error::Error>>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut conn = DbConnection::establish(db).await?;
        apply_migrations(&mut conn, db).await?;
        f(&mut conn).await
    })
}

/// Populate the database with sample files and ACLs for file-related tests.
pub fn setup_files_db(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    with_db(db, |conn| {
        Box::pin(async move {
            let argon2 = argon2::Argon2::default();
            let hashed = hash_password(&argon2, "secret")?;
            let new_user = NewUser {
                username: "alice",
                password: &hashed,
            };
            create_user(conn, &new_user).await?;
            let files = [
                NewFileEntry {
                    name: "fileA.txt",
                    object_key: "1",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileB.txt",
                    object_key: "2",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileC.txt",
                    object_key: "3",
                    size: 1,
                },
            ];
            for file in &files {
                create_file(conn, file).await?;
            }
            let acls = [
                NewFileAcl {
                    file_id: 1,
                    user_id: 1,
                },
                NewFileAcl {
                    file_id: 3,
                    user_id: 1,
                },
            ];
            for acl in &acls {
                add_file_acl(conn, acl).await?;
            }
            Ok(())
        })
    })
}

/// Populate the database with a "General" category and a couple of articles.
pub fn setup_news_db(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    with_db(db, |conn| {
        Box::pin(async move {
            create_category(
                conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            use mxd::schema::news_articles::dsl as a;
            let posted = DateTime::<Utc>::from_timestamp(1000, 0)
                .expect("valid timestamp")
                .naive_utc();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: None,
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "First",
                    poster: None,
                    posted_at: posted,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("a"),
                })
                .execute(conn)
                .await?;
            let posted2 = DateTime::<Utc>::from_timestamp(2000, 0)
                .expect("valid timestamp")
                .naive_utc();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: Some(1),
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "Second",
                    poster: None,
                    posted_at: posted2,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("b"),
                })
                .execute(conn)
                .await?;
            Ok(())
        })
    })
}

/// Populate the database with a bundle and two categories at the root level.
pub fn setup_news_categories_root_db(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    with_db(db, |conn| {
        Box::pin(async move {
            use mxd::db::create_category;

            let _ = insert_root_bundle(conn).await?;

            let _ = create_category(
                conn,
                &mxd::models::NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            let _ = create_category(
                conn,
                &mxd::models::NewCategory {
                    name: "Updates",
                    bundle_id: None,
                },
            )
            .await?;
            Ok(())
        })
    })
}

/// Populate the database with a nested bundle containing a single category.
pub fn setup_news_categories_nested_db(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    with_db(db, |conn| {
        Box::pin(async move {
            use mxd::{
                db::{create_bundle, create_category},
                models::NewBundle,
            };

            let root_id = insert_root_bundle(conn).await?;

            let sub_id = create_bundle(
                conn,
                &NewBundle {
                    parent_bundle_id: Some(root_id),
                    name: "Sub",
                },
            )
            .await?;

            let _ = create_category(
                conn,
                &mxd::models::NewCategory {
                    name: "Inside",
                    bundle_id: Some(sub_id),
                },
            )
            .await?;
            Ok(())
        })
    })
}

async fn insert_root_bundle(conn: &mut DbConnection) -> Result<i32, Box<dyn std::error::Error>> {
    use mxd::{db::create_bundle, models::NewBundle};

    let id = create_bundle(
        conn,
        &NewBundle {
            parent_bundle_id: None,
            name: "Bundle",
        },
    )
    .await?;

    Ok(id)
}
