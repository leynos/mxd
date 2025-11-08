//! Utilities for integration tests.
//!
//! The `test-util` crate provides helpers to spin up temporary servers and,
//! when the `postgres` feature is enabled, manage embedded PostgreSQL
//! instances. It is used by integration tests in the main crate.
use std::{
    ffi::OsString,
    io::{BufRead, BufReader},
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use tempfile::TempDir;

pub type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "postgres")]
pub use postgres::{PostgresTestDb, postgres_db};

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("Either feature 'sqlite' or 'postgres' must be enabled");

#[inline]
fn ensure_single_backend() {
    assert!(
        !cfg!(all(feature = "sqlite", feature = "postgres")),
        "Choose either sqlite or postgres, not both",
    );
}

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
    db: PostgresTestDb,
    /// Keep the temporary directory alive for the lifetime of the server to
    /// prevent early cleanup while the database is running. For PostgreSQL the
    /// temporary directory is managed by `PostgresTestDb` and this field remains
    /// `None`.
    temp_dir: Option<TempDir>,
}

// No additional resource wrapper needed; `PostgresTestDb` handles
// temporary database setup and teardown for tests.

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
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn setup_sqlite<F>(temp: &TempDir, setup: F) -> Result<String, AnyError>
where
    F: FnOnce(&str) -> Result<(), AnyError>,
{
    let path = temp.path().join("mxd.db");
    setup(path.to_str().expect("db path utf8"))?;
    Ok(path.to_str().unwrap().to_owned())
}

fn wait_for_server(child: &mut Child) -> Result<(), AnyError> {
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
    // Use the same cargo executable as the parent process for consistency
    // and to allow environments without `cargo` in `PATH`. `var_os` handles
    // potentially non-UTF-8 paths.
    let cargo: OsString = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut cmd = Command::new(cargo);
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

fn launch_server_process(manifest_path: &str, db_url: &str) -> Result<(Child, u16), AnyError> {
    let socket = TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);

    let mut child = build_server_command(manifest_path, port, db_url).spawn()?;
    wait_for_server(&mut child)?;
    Ok((child, port))
}

impl TestServer {
    /// Start the server using the given Cargo manifest path.
    pub fn start(manifest_path: &str) -> Result<Self, AnyError> {
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
    pub fn start_with_setup<F>(manifest_path: &str, setup: F) -> Result<Self, AnyError>
    where
        F: FnOnce(&str) -> Result<(), AnyError>,
    {
        ensure_single_backend();
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        {
            let temp = TempDir::new()?;
            let db_url = setup_sqlite(&temp, setup)?;
            return Self::launch(manifest_path, db_url, Some(temp));
        }

        #[cfg(feature = "postgres")]
        {
            let db = crate::postgres::PostgresTestDb::new()?;
            setup(db.url.as_ref())?;
            return Self::launch(manifest_path, db);
        }
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    fn launch(
        manifest_path: &str,
        db_url: String,
        temp_dir: Option<TempDir>,
    ) -> Result<Self, AnyError> {
        let (child, port) = launch_server_process(manifest_path, &db_url)?;
        Ok(Self {
            child,
            port,
            db_url,
            temp_dir,
        })
    }

    #[cfg(feature = "postgres")]
    fn launch(manifest_path: &str, db: PostgresTestDb) -> Result<Self, AnyError> {
        let db_url = db.url.to_string();
        let (child, port) = launch_server_process(manifest_path, &db_url)?;
        Ok(Self {
            child,
            port,
            db_url,
            db,
            temp_dir: None,
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
    pub fn uses_embedded_postgres(&self) -> bool { self.db.uses_embedded() }
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
        {
            // `PostgresTestDb` cleans up the database and stops the embedded
            // server in its `Drop` implementation.
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
pub fn with_db<F>(db: &str, f: F) -> Result<(), AnyError>
where
    F: for<'c> FnOnce(&'c mut DbConnection) -> BoxFuture<'c, Result<(), AnyError>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut conn = DbConnection::establish(db).await?;
        apply_migrations(&mut conn, db).await?;
        f(&mut conn).await
    })
}

/// Populate the database with sample files and ACLs for file-related tests.
pub fn setup_files_db(db: &str) -> Result<(), AnyError> {
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
pub fn setup_news_db(db: &str) -> Result<(), AnyError> {
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
pub fn setup_news_categories_root_db(db: &str) -> Result<(), AnyError> {
    setup_news_categories_with_structure(db, |conn, _| {
        Box::pin(async move {
            use mxd::db::create_category;

            create_category(
                conn,
                &mxd::models::NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            create_category(
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
pub fn setup_news_categories_nested_db(db: &str) -> Result<(), AnyError> {
    setup_news_categories_with_structure(db, |conn, root_id| {
        Box::pin(async move {
            use mxd::{
                db::{create_bundle, create_category},
                models::NewBundle,
            };

            let sub_id = create_bundle(
                conn,
                &NewBundle {
                    parent_bundle_id: Some(root_id),
                    name: "Sub",
                },
            )
            .await?;

            create_category(
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

/// Build the common news category tree then invoke a custom builder.
///
/// Creates the root bundle shared across tests and runs the provided closure to
/// add additional bundles or categories. Returns an error if database setup
/// fails at any stage.
pub fn setup_news_categories_with_structure<F>(db: &str, build: F) -> Result<(), AnyError>
where
    F: 'static
        + Send
        + for<'c> FnOnce(&'c mut DbConnection, i32) -> BoxFuture<'c, Result<(), AnyError>>,
{
    with_db(db, |conn| {
        Box::pin(async move {
            let root_id = insert_root_bundle(conn).await?;
            build(conn, root_id).await
        })
    })
}

async fn insert_root_bundle(conn: &mut DbConnection) -> Result<i32, AnyError> {
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
