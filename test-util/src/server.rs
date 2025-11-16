//! Test server harness used by integration suites.
//!
//! Provides helpers to launch the `mxd` server binary with either the SQLite or
//! PostgreSQL backend, monitor readiness, and tear it down once tests complete.

use std::{
    ffi::OsString,
    fmt,
    io::{self, BufRead, BufReader},
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

#[cfg(unix)]
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use tempfile::TempDir;

use crate::AnyError;
#[cfg(feature = "postgres")]
use crate::postgres::PostgresTestDb;

/// Newtype wrapping the path to a Cargo manifest, providing type-safe handling
/// and ergonomic conversions.
#[derive(Debug, Clone)]
pub struct ManifestPath(String);

impl ManifestPath {
    /// Constructs a new manifest path from any string-like type.
    pub fn new(path: impl Into<String>) -> Self { Self(path.into()) }
    /// Returns the path as a string slice.
    pub fn as_str(&self) -> &str { &self.0 }
}

impl From<&str> for ManifestPath {
    fn from(value: &str) -> Self { Self(value.to_owned()) }
}

impl From<String> for ManifestPath {
    fn from(value: String) -> Self { Self(value) }
}

impl AsRef<str> for ManifestPath {
    fn as_ref(&self) -> &str { &self.0 }
}

impl AsRef<Path> for ManifestPath {
    fn as_ref(&self) -> &Path { Path::new(&self.0) }
}

impl fmt::Display for ManifestPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

/// Newtype wrapping a database connection URL that provides ergonomic
/// conversions for type-safe handling.
#[derive(Debug, Clone)]
pub struct DbUrl(String);

impl DbUrl {
    /// Constructs a new database URL from any string-like type.
    pub fn new(url: impl Into<String>) -> Self { Self(url.into()) }
    /// Returns the URL as a string slice.
    pub fn as_str(&self) -> &str { &self.0 }
}

impl From<&str> for DbUrl {
    fn from(value: &str) -> Self { Self(value.to_owned()) }
}

impl From<String> for DbUrl {
    fn from(value: String) -> Self { Self(value) }
}

impl AsRef<str> for DbUrl {
    fn as_ref(&self) -> &str { &self.0 }
}

impl fmt::Display for DbUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Ensure `CARGO_BIN_EXE_mxd` is populated from the provided compile-time path.
///
/// The mutation is guarded by a global mutex and the result is propagated so
/// callers can handle synchronisation failures instead of panicking.
pub fn ensure_server_binary_env(bin_path: &str) -> Result<(), AnyError> {
    let _guard = ENV_LOCK
        .lock()
        .map_err(|_| io::Error::other("environment mutex poisoned"))?;
    if std::env::var_os("CARGO_BIN_EXE_mxd").is_none() {
        // SAFETY: Environment mutation is serialized by `ENV_LOCK`, ensuring no
        // concurrent readers/writers observe a partially updated state.
        unsafe { std::env::set_var("CARGO_BIN_EXE_mxd", bin_path) };
    }
    Ok(())
}

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("Either feature 'sqlite' or 'postgres' must be enabled");

#[inline]
fn ensure_single_backend() {
    const {
        assert!(
            !cfg!(all(feature = "sqlite", feature = "postgres")),
            "Choose either sqlite or postgres, not both",
        );
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn setup_sqlite<F>(temp: &TempDir, setup: F) -> Result<DbUrl, AnyError>
where
    F: FnOnce(&DbUrl) -> Result<(), AnyError>,
{
    let path = temp.path().join("mxd.db");
    let path_str = path
        .to_str()
        .ok_or_else(|| "database path is not valid UTF-8".to_string())?;
    let url = DbUrl::from(path_str);
    setup(&url)?;
    Ok(url)
}

/// Waits up to ten seconds for the child `mxd` process to announce readiness
/// on stdout, returning an error if it exits early or never signals.
fn wait_for_server(child: &mut Child) -> Result<(), AnyError> {
    if let Some(out) = &mut child.stdout {
        let mut reader = BufReader::new(out);
        let mut line = String::new();
        let timeout = Duration::from_secs(10);
        let start = Instant::now();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                return Err("server exited before signalling readiness".into());
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

/// Constructs the base `cargo run` command for launching the server with the
/// requested manifest, bind port, and database URL, enabling the active backend.
fn build_server_command(manifest_path: &ManifestPath, port: u16, db_url: &DbUrl) -> Command {
    if let Some(bin) = std::env::var_os("CARGO_BIN_EXE_mxd") {
        return server_binary_command(bin, port, db_url);
    }
    cargo_run_command(manifest_path, port, db_url)
}

/// Builds a command that executes an already-built `mxd` binary bound to the
/// requested port and database URL, bypassing `cargo run` entirely.
fn server_binary_command(bin: OsString, port: u16, db_url: &DbUrl) -> Command {
    let mut cmd = Command::new(bin);
    cmd.arg("--bind");
    cmd.arg(format!("127.0.0.1:{port}"));
    cmd.arg("--database");
    cmd.arg(db_url.as_str());
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());
    cmd
}

/// Produces a `cargo run` invocation tailored to the active backend, falling
/// back to this path when no prebuilt binary is available.
fn cargo_run_command(manifest_path: &ManifestPath, port: u16, db_url: &DbUrl) -> Command {
    let cargo: OsString = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut cmd = Command::new(cargo);
    cmd.arg("run");
    #[cfg(feature = "postgres")]
    {
        cmd.args(["--no-default-features", "--features", "postgres"]);
    }
    #[cfg(feature = "sqlite")]
    {
        cmd.args(["--features", "sqlite"]);
    }
    // Ensure the server binary matches the feature set used by tests so Cargo
    // does not trigger a costly rebuild when the harness falls back to
    // `cargo run` (for example when `CARGO_BIN_EXE_mxd` is unavailable).
    cmd.args(["--features", "test-support"]);
    cmd.args([
        "--bin",
        "mxd",
        "--manifest-path",
        manifest_path.as_str(),
        "--quiet",
        "--",
        "--bind",
        &format!("127.0.0.1:{port}"),
        "--database",
        db_url.as_str(),
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit());
    cmd
}

/// Spawns the configured server process on an ephemeral port and waits for the
/// readiness banner before returning the child handle and chosen port.
fn launch_server_process(
    manifest_path: &ManifestPath,
    db_url: &DbUrl,
) -> Result<(Child, u16), AnyError> {
    let socket = TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);

    let mut child = build_server_command(manifest_path, port, db_url).spawn()?;
    if let Err(e) = wait_for_server(&mut child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    Ok((child, port))
}

pub struct TestServer {
    child: Child,
    port: u16,
    db_url: DbUrl,
    #[cfg(feature = "postgres")]
    db: PostgresTestDb,
    temp_dir: Option<TempDir>,
}

impl TestServer {
    pub fn start(manifest_path: impl Into<ManifestPath>) -> Result<Self, AnyError> {
        Self::start_with_setup(manifest_path, |_| Ok(()))
    }

    pub fn start_with_setup<F>(
        manifest_path: impl Into<ManifestPath>,
        setup: F,
    ) -> Result<Self, AnyError>
    where
        F: FnOnce(&DbUrl) -> Result<(), AnyError>,
    {
        let manifest_path = manifest_path.into();
        ensure_single_backend();
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        {
            let temp = TempDir::new()?;
            let db_url = setup_sqlite(&temp, setup)?;
            Self::launch(&manifest_path, db_url, Some(temp))
        }

        #[cfg(feature = "postgres")]
        {
            let db = crate::postgres::PostgresTestDb::new()?;
            let db_url = DbUrl::from(db.url.as_ref());
            setup(&db_url)?;
            Self::launch(&manifest_path, db, db_url)
        }
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    fn launch(
        manifest_path: &ManifestPath,
        db_url: DbUrl,
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
    fn launch(
        manifest_path: &ManifestPath,
        db: PostgresTestDb,
        db_url: DbUrl,
    ) -> Result<Self, AnyError> {
        let (child, port) = launch_server_process(manifest_path, &db_url)?;
        Ok(Self {
            child,
            port,
            db_url,
            db,
            temp_dir: None,
        })
    }

    pub fn port(&self) -> u16 { self.port }

    pub fn db_url(&self) -> &DbUrl { &self.db_url }

    pub fn temp_dir(&self) -> Option<&TempDir> { self.temp_dir.as_ref() }

    #[cfg(feature = "postgres")]
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
    }
}
