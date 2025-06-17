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
    _temp: Option<TempDir>,
    #[cfg(feature = "postgres")]
    pg: Option<PostgreSQL>,
}

#[cfg(feature = "postgres")]
fn start_embedded_postgres<F>(setup: F) -> Result<(String, PostgreSQL), Box<dyn std::error::Error>>
where
    F: FnOnce(&str) -> Result<(), Box<dyn std::error::Error>>,
{
    let fut = async {
        let mut settings = Settings::default();
        let (tmp, data_dir) = tempfile::tempdir()?.keep()?;
        std::mem::forget(tmp);
        settings.data_dir = data_dir.clone();

        let mut pg = if geteuid().is_root() {
            let bin = HELPER_BIN.clone().map_err(|e| e.into())?;

            let run_status = std::process::Command::new(bin)
                .env("PG_DATA_DIR", &data_dir)
                .env("PG_PORT", settings.port.to_string())
                .env("PG_VERSION_REQ", settings.version.to_string())
                .env("PG_SUPERUSER", &settings.username)
                .env("PG_PASSWORD", &settings.password)
                .status()
                .map_err(|e| format!("running postgres-setup-unpriv: {e}"))?;
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
        if let Err(e) = pg.create_database("test").await {
            let _ = pg.stop().await;
            return Err(format!("creating test database: {e}").into());
        }
        let url = pg.settings().url("test");
        Ok::<_, Box<dyn StdError>>((url, pg))
    };
    let (url, pg) = match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.block_on(fut)?,
        Err(_) => tokio::runtime::Runtime::new()?.block_on(fut)?,
    };
    setup(&url)?;
    Ok((url, pg))
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

/// RAII-style PostgreSQL test database fixture that ensures clean schema state.
///
/// This struct manages the lifecycle of a PostgreSQL database for testing, automatically
/// handling schema reset both on creation and destruction. It follows the RAII pattern
/// to guarantee that each test gets a pristine database schema regardless of how the
/// previous test terminated.
///
/// The database can be either:
/// - An external PostgreSQL instance specified via `POSTGRES_TEST_URL` environment variable
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
/// Set the `POSTGRES_TEST_URL` environment variable to reuse an existing PostgreSQL instance:
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
}

#[cfg(feature = "postgres")]
impl PostgresTestDb {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(value) = std::env::var_os("POSTGRES_TEST_URL") {
            let url = value.to_string_lossy().into_owned();
            reset_postgres_db(&url)?;
            return Ok(Self { url, pg: None });
        }

        let (url, pg) = start_embedded_postgres(|url| reset_postgres_db(url))?;
        Ok(Self { url, pg: Some(pg) })
    }

    pub fn uses_embedded(&self) -> bool { self.pg.is_some() }
}

#[cfg(feature = "postgres")]
impl Drop for PostgresTestDb {
    fn drop(&mut self) {
        let _ = reset_postgres_db(&self.url);
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
/// - **External Database**: If `POSTGRES_TEST_URL` is set, uses that database
/// - **Embedded Database**: Otherwise, starts an embedded PostgreSQL server
/// - **Schema Reset**: Drops and recreates the `public` schema before each test
/// - **Cleanup**: Automatically resets schema and stops embedded server after each test
///
/// ## Environment Variable
///
/// Set `POSTGRES_TEST_URL` to reuse an existing PostgreSQL instance:
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
            let (db_url, pg) = if let Some(value) = std::env::var_os("POSTGRES_TEST_URL") {
                let url = value.to_string_lossy().into_owned();
                reset_postgres_db(&url)?;
                (url, None)
            } else {
                let (url, pg) = start_embedded_postgres(|url| reset_postgres_db(url))?;
                (url, Some(pg))
            };
            setup(&db_url)?;
            return Self::launch(manifest_path, db_url, None, pg);
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
        temp: Option<TempDir>,
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
            _temp: temp,
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
        temp: Option<TempDir>,
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
            _temp: temp,
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
