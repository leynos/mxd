//! Test server harness used by integration suites.
//!
//! Provides helpers to launch the `mxd` server binary with either the `SQLite` or
//! `PostgreSQL` backend, monitor readiness, and tear it down once tests complete.

use std::{
    collections::VecDeque,
    ffi::OsString,
    fmt,
    io::{self, BufRead, BufReader},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

#[cfg(unix)]
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use tempfile::TempDir;
use tracing::{debug, info, warn};

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

/// Name of the server binary to use for integration tests.
///
/// The wireframe server (`mxd-wireframe-server`) is the default, as it provides
/// the production-ready transport layer implementation.
const SERVER_BINARY_NAME: &str = "mxd-wireframe-server";

/// Environment variable name for the prebuilt server binary path.
const SERVER_BINARY_ENV: &str = "CARGO_BIN_EXE_mxd-wireframe-server";
/// Marker emitted on stdout to signal that the server is ready.
const READY_MARKER: &str = "listening on";

/// Ensure the server binary environment variable is populated from the provided
/// compile-time path.
///
/// The mutation is guarded by a global mutex and the result is propagated so
/// callers can handle synchronisation failures instead of panicking.
///
/// # Errors
///
/// Returns an error if the environment mutex is poisoned.
pub fn ensure_server_binary_env(bin_path: &str) -> Result<(), AnyError> {
    let _guard = ENV_LOCK
        .lock()
        .map_err(|_| io::Error::other("environment mutex poisoned"))?;
    if std::env::var_os(SERVER_BINARY_ENV).is_none() {
        // SAFETY: Environment mutation is serialized by `ENV_LOCK`, ensuring no
        // concurrent readers/writers observe a partially updated state.
        unsafe { std::env::set_var(SERVER_BINARY_ENV, bin_path) };
    }
    Ok(())
}

struct EnvVarGuard {
    key: String,
    previous: Option<OsString>,
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => {
                // SAFETY: Environment mutation is serialized by `ENV_LOCK`.
                unsafe { std::env::set_var(&self.key, value) };
            }
            None => {
                // SAFETY: Environment mutation is serialized by `ENV_LOCK`.
                unsafe { std::env::remove_var(&self.key) };
            }
        }
    }
}

/// Temporarily sets an environment variable for the duration of a closure.
///
/// The mutation is guarded by a global mutex and is restored after the closure
/// returns, even when it returns an error.
///
/// # Errors
///
/// Returns an error if the environment mutex is poisoned or the closure fails.
pub fn with_env_var<T>(
    key: &str,
    value: Option<&str>,
    f: impl FnOnce() -> Result<T, AnyError>,
) -> Result<T, AnyError> {
    let _guard = ENV_LOCK
        .lock()
        .map_err(|_| io::Error::other("environment mutex poisoned"))?;
    let previous = std::env::var_os(key);
    match value {
        Some(env_value) => {
            // SAFETY: Environment mutation is serialized by `ENV_LOCK`.
            unsafe { std::env::set_var(key, env_value) };
        }
        None => {
            // SAFETY: Environment mutation is serialized by `ENV_LOCK`.
            unsafe { std::env::remove_var(key) };
        }
    }
    let restore = EnvVarGuard {
        key: key.to_owned(),
        previous,
    };
    let result = f();
    drop(restore);
    result
}

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("Either feature 'sqlite' or 'postgres' must be enabled");

// NOTE: The mutual exclusion of sqlite/postgres is NOT enforced at compile time
// when `--all-features` is used (e.g., by `make lint`). The `#[cfg(...)]` guards
// on `setup_sqlite` and the postgres launch path ensure correct behavior at
// runtime. This design allows the crate to pass workspace-wide clippy checks.
#[expect(
    clippy::missing_const_for_fn,
    reason = "inline hint for call-site cfg guards; const not needed"
)]
#[inline]
fn ensure_single_backend() {
    // Intentionally empty - cfg guards handle feature selection at compile time
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
/// Creates a temporary `SQLite` database at `temp/mxd.db`, runs the provided
/// setup callback with its URL, and returns that URL on success. The callback
/// must implement `FnOnce(&DbUrl) -> Result<(), AnyError>`. Returns an error if
/// the path is not valid UTF-8 or if the callback fails.
fn setup_sqlite<F>(temp: &TempDir, setup: F) -> Result<DbUrl, AnyError>
where
    F: FnOnce(&DbUrl) -> Result<(), AnyError>,
{
    let path = temp.path().join("mxd.db");
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("database path is not valid UTF-8"))?;
    let url = DbUrl::from(path_str);
    setup(&url)?;
    Ok(url)
}

fn collect_stdout_for_diagnostics(
    reader: &mut BufReader<&mut ChildStdout>,
    max_lines: usize,
    timeout: Duration,
) -> Result<(Vec<String>, bool), AnyError> {
    let mut lines = VecDeque::with_capacity(max_lines);
    let mut line = String::new();
    let start = Instant::now();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok((lines.into_iter().collect(), false));
        }
        record_diagnostic_line(&mut lines, max_lines, line.trim());
        if line.contains(READY_MARKER) {
            return Ok((lines.into_iter().collect(), true));
        }
        if start.elapsed() > timeout {
            return Ok((lines.into_iter().collect(), false));
        }
    }
}

fn record_diagnostic_line(lines: &mut VecDeque<String>, max_lines: usize, line: &str) {
    if max_lines == 0 {
        return;
    }
    if lines.len() == max_lines {
        lines.pop_front();
    }
    lines.push_back(line.to_owned());
}

/// Waits up to ten seconds for the child `mxd` process to announce readiness
/// on stdout, returning an error if it exits early or never signals.
fn wait_for_server(child: &mut Child) -> Result<(), AnyError> {
    if let Some(out) = &mut child.stdout {
        let mut reader = BufReader::new(out);
        let timeout = Duration::from_secs(10);
        let (lines_received, ready) = collect_stdout_for_diagnostics(&mut reader, 50, timeout)?;
        if ready {
            Ok(())
        } else {
            warn!(lines_received = ?lines_received, "server did not signal readiness");
            Err(anyhow::anyhow!("server failed to signal readiness"))
        }
    } else {
        Err(anyhow::anyhow!("missing stdout from server"))
    }
}

fn resolve_server_binary() -> Option<PathBuf> {
    let resolution =
        std::env::var_os(SERVER_BINARY_ENV).map_or(ServerBinaryResolution::EnvMissing, |bin| {
            let path = PathBuf::from(bin);
            if path.exists() {
                ServerBinaryResolution::Found(path)
            } else {
                ServerBinaryResolution::Missing(path)
            }
        });
    resolution.log();
    resolution.into_option()
}

enum ServerBinaryResolution {
    EnvMissing,
    Found(PathBuf),
    Missing(PathBuf),
}

impl ServerBinaryResolution {
    fn log(&self) {
        let (message, binary) = match self {
            Self::EnvMissing => ("env var not set", None),
            Self::Found(path) => ("using prebuilt binary", Some(path.as_path())),
            Self::Missing(path) => ("binary from env var does not exist", Some(path.as_path())),
        };
        log_server_binary_resolution(message, binary);
    }

    fn into_option(self) -> Option<PathBuf> {
        match self {
            Self::Found(path) => Some(path),
            Self::EnvMissing | Self::Missing(_) => None,
        }
    }
}

fn log_server_binary_resolution(message: &'static str, binary: Option<&Path>) {
    let binary_display = binary.map(|path| path.display().to_string());
    debug!(
        env_var = SERVER_BINARY_ENV,
        binary = ?binary_display,
        "{message}"
    );
}

/// Constructs the base `cargo run` command for launching the server with the
/// requested manifest, bind port, and database URL, enabling the active backend.
fn build_server_command(manifest_path: &ManifestPath, port: u16, db_url: &DbUrl) -> Command {
    if let Some(bin) = resolve_server_binary() {
        return server_binary_command(bin, port, db_url);
    }
    debug!("falling back to cargo run");
    cargo_run_command(manifest_path, port, db_url)
}

/// Builds a command that executes an already-built wireframe server binary bound
/// to the requested port and database URL, bypassing `cargo run` entirely.
fn server_binary_command(bin: PathBuf, port: u16, db_url: &DbUrl) -> Command {
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
    // Always use --no-default-features and explicitly specify required features
    // to ensure the binary is built with the same feature set as the tests.
    cmd.arg("--no-default-features");
    #[cfg(feature = "postgres")]
    {
        cmd.args(["--features", "postgres"]);
    }
    #[cfg(feature = "sqlite")]
    {
        // Keep sqlite builds aligned with default features: Cargo.toml defines
        // `toml` (figment/toml + dep:toml) for configuration/fixture parsing,
        // so we pass `--features sqlite,toml` to ensure compilation matches.
        cmd.args(["--features", "sqlite,toml"]);
    }
    // Ensure the server binary matches the feature set used by tests so Cargo
    // does not trigger a costly rebuild when the harness falls back to
    // `cargo run` (for example when the prebuilt binary is unavailable).
    cmd.args(["--features", "test-support"]);
    cmd.args([
        "--bin",
        SERVER_BINARY_NAME,
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
#[expect(
    clippy::cognitive_complexity,
    reason = "process spawning with cleanup on failure has inherent complexity"
)]
#[expect(
    clippy::let_underscore_must_use,
    reason = "best-effort cleanup; error already being propagated"
)]
fn launch_server_process(
    manifest_path: &ManifestPath,
    db_url: &DbUrl,
) -> Result<(Child, u16), AnyError> {
    let socket = TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);

    info!(port, db_url = %db_url, "launching server");
    let mut child = build_server_command(manifest_path, port, db_url).spawn()?;
    debug!("spawned server process, waiting for readiness");
    if let Err(e) = wait_for_server(&mut child) {
        warn!(error = %e, "wait_for_server failed");
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    info!(port, "server ready");
    Ok((child, port))
}

/// Integration test server wrapper that spawns the `mxd` process with the
/// selected backend, waits for readiness, and tears it down automatically on
/// drop.
pub struct TestServer {
    child: Child,
    port: u16,
    db_url: DbUrl,
    #[cfg(feature = "postgres")]
    db: PostgresTestDb,
    temp_dir: Option<TempDir>,
}

impl TestServer {
    /// Launches a server with the default (empty) setup, returning an error if
    /// the database or server cannot be initialised or readiness times out (ten
    /// seconds).
    ///
    /// # Errors
    ///
    /// Returns an error if database or server initialisation fails.
    pub fn start(manifest_path: impl Into<ManifestPath>) -> Result<Self, AnyError> {
        Self::start_with_setup(manifest_path, |_| Ok(()))
    }

    /// Launches a server and runs the setup callback with the database URL
    /// before starting, useful for seeding data or running migrations; returns
    /// an error if setup, database initialisation, or launch fails.
    ///
    /// # Errors
    ///
    /// Returns an error if setup, database initialisation, or launch fails.
    #[expect(clippy::shadow_reuse, reason = "standard Into pattern")]
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

    /// Returns the ephemeral port on which the server is listening.
    #[must_use]
    pub const fn port(&self) -> u16 { self.port }

    /// Returns the database URL used by the server.
    #[must_use]
    pub const fn db_url(&self) -> &DbUrl { &self.db_url }

    /// Returns the temporary directory holding the `SQLite` database, if
    /// applicable. Returns `None` when using `PostgreSQL`.
    #[must_use]
    pub const fn temp_dir(&self) -> Option<&TempDir> { self.temp_dir.as_ref() }

    #[cfg(feature = "postgres")]
    /// Returns `true` when the server is using an embedded `PostgreSQL` instance
    /// rather than an external server.
    #[must_use]
    pub const fn uses_embedded_postgres(&self) -> bool { self.db.uses_embedded() }
}

impl Drop for TestServer {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "best-effort cleanup; Drop cannot propagate errors"
    )]
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            #[expect(
                clippy::cast_possible_wrap,
                reason = "process IDs won't exceed i32::MAX on supported platforms"
            )]
            let _ = kill(Pid::from_raw(self.child.id() as i32), Signal::SIGTERM);
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
