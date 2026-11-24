//! Test server harness used by integration suites.
//!
//! Provides helpers to launch the `mxd` server binary with either the `SQLite` or
//! `PostgreSQL` backend, monitor readiness, and tear it down once tests complete.

use std::{
    ffi::OsString,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

mod binary;
mod env;
mod readiness;
mod target;

pub use env::{DbUrl, ManifestPath};
#[cfg(unix)]
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use readiness::wait_for_server;
use target::ServerTarget;
use tempfile::TempDir;
use tracing::{debug, info, warn};

use crate::AnyError;
#[cfg(feature = "postgres")]
use crate::postgres::PostgresTestDb;

/// Name of the server binary to use for integration tests.
///
/// The wireframe server (`mxd-wireframe-server`) is the default, as it provides
/// the production-ready transport layer implementation.
const SERVER_BINARY_NAME: &str = "mxd-wireframe-server";
const DEFAULT_BIND_HOST: &str = "127.0.0.1";
const TEST_BIND_HOST_ENV: &str = "MXD_TEST_BIND_HOST";
const MAX_SERVER_LAUNCH_ATTEMPTS: u8 = 3;

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("Either feature 'sqlite' or 'postgres' must be enabled");

// NOTE: The mutual exclusion of sqlite/postgres is NOT enforced at compile time
// when `--all-features` is used (e.g., by `make lint`). The `#[cfg(...)]` guards
// on `setup_sqlite` and the postgres launch path ensure correct behaviour at
// runtime. This design allows the crate to pass workspace-wide clippy checks.
#[expect(
    clippy::missing_const_for_fn,
    reason = "inline hint for call-site cfg guards; const not needed"
)]
#[inline]
fn ensure_single_backend() {
    // Intentionally empty - cfg guards handle feature selection at compile time
}

/// Creates a temporary `SQLite` database at `temp/mxd.db`, runs the provided
/// setup callback with its URL, and returns that URL on success. The callback
/// must implement `FnOnce(&DbUrl) -> Result<(), AnyError>`. Returns an error if
/// the path is not valid UTF-8 or if the callback fails.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
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

/// Constructs the base `cargo run` command for launching the server with the
/// requested manifest, bind address, and database URL, enabling the active backend.
fn build_server_command(target: &ServerTarget, addr: SocketAddr, db_url: &DbUrl) -> Command {
    if let Some(bin) = target.binary() {
        return server_binary_command(bin, addr, db_url);
    }
    debug!("falling back to cargo run");
    cargo_run_command(target.manifest_path(), addr, db_url)
}

/// Builds a command that executes an already-built wireframe server binary bound
/// to the requested address and database URL, bypassing `cargo run` entirely.
fn server_binary_command(bin: PathBuf, addr: SocketAddr, db_url: &DbUrl) -> Command {
    let mut cmd = Command::new(bin);
    cmd.arg("--bind");
    cmd.arg(addr.to_string());
    cmd.arg("--database");
    cmd.arg(db_url.as_str());
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());
    cmd
}

/// Produces a `cargo run` invocation tailored to the active backend, falling
/// back to this path when no prebuilt binary is available.
fn cargo_run_command(manifest_path: &ManifestPath, addr: SocketAddr, db_url: &DbUrl) -> Command {
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
        // Keep sqlite builds aligned with default features: the root `toml`
        // feature forwards TOML parsing to OrthoConfig and `cli-defs`, so pass
        // it explicitly to ensure the server has the same configuration path.
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
        &addr.to_string(),
        "--database",
        db_url.as_str(),
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit());
    cmd
}

fn resolve_bind_host() -> Result<String, AnyError> {
    let value =
        std::env::var_os(TEST_BIND_HOST_ENV).unwrap_or_else(|| OsString::from(DEFAULT_BIND_HOST));
    value
        .into_string()
        .map_err(|_| anyhow::anyhow!("{TEST_BIND_HOST_ENV} must be valid UTF-8"))
}

/// Maps an unspecified bind address to the loopback address that reaches it.
const fn readiness_address(addr: SocketAddr) -> SocketAddr {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
        }
        IpAddr::V6(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), addr.port())
        }
        _ => addr,
    }
}

/// Picks a free port on `bind_host` and releases it for the server to claim.
fn reserve_ephemeral_addr(bind_host: &str) -> Result<SocketAddr, AnyError> {
    let socket = TcpListener::bind((bind_host, 0))?;
    let addr = socket.local_addr()?;
    drop(socket);
    Ok(addr)
}

/// Kills and reaps a server that never became ready.
#[expect(
    clippy::let_underscore_must_use,
    reason = "best-effort cleanup; error already being propagated"
)]
fn abandon_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The result of one launch attempt that got as far as spawning the server.
enum LaunchOutcome {
    /// The server accepted a connection.
    Ready(Child, SocketAddr),
    /// The server never became ready, and has been killed and reaped.
    NotReady(AnyError),
}

/// Starts the server process listening on `addr`.
#[expect(
    clippy::cognitive_complexity,
    reason = "the tracing macros expand to branches that the lint counts"
)]
fn spawn_server(
    target: &ServerTarget,
    addr: SocketAddr,
    db_url: &DbUrl,
    attempt: u8,
) -> Result<Child, AnyError> {
    info!(
        port = addr.port(),
        db_url = %db_url,
        attempt,
        "launching server"
    );
    let child = build_server_command(target, addr, db_url).spawn()?;
    debug!("spawned server process, waiting for readiness");
    Ok(child)
}

/// Waits for `child` to accept connections, killing and reaping it on failure.
#[expect(
    clippy::cognitive_complexity,
    reason = "the tracing macros expand to branches that the lint counts"
)]
fn await_readiness(mut child: Child, addr: SocketAddr, attempt: u8) -> LaunchOutcome {
    match wait_for_server(&mut child, readiness_address(addr)) {
        Ok(()) => {
            info!(port = addr.port(), attempt, "server ready");
            LaunchOutcome::Ready(child, addr)
        }
        Err(error) => {
            warn!(error = %error, attempt, "wait_for_server failed");
            abandon_child(&mut child);
            LaunchOutcome::NotReady(error)
        }
    }
}

/// Spawns the server once and waits for its socket to accept connections.
///
/// A failure to reserve a port or spawn the process is returned as an error and
/// is not retried; a server that spawns but never becomes ready is reported as
/// [`LaunchOutcome::NotReady`] so the caller can retry.
fn launch_server_once(
    target: &ServerTarget,
    bind_host: &str,
    db_url: &DbUrl,
    attempt: u8,
) -> Result<LaunchOutcome, AnyError> {
    let addr = reserve_ephemeral_addr(bind_host)?;
    let child = spawn_server(target, addr, db_url, attempt)?;
    Ok(await_readiness(child, addr, attempt))
}

/// Spawns the configured server process on an ephemeral port and waits for the
/// socket to accept connections before returning the child handle and chosen
/// port.
fn launch_server_process(
    target: &ServerTarget,
    bind_host: &str,
    db_url: &DbUrl,
) -> Result<(Child, SocketAddr), AnyError> {
    for attempt in 1..=MAX_SERVER_LAUNCH_ATTEMPTS {
        match launch_server_once(target, bind_host, db_url, attempt)? {
            LaunchOutcome::Ready(child, addr) => return Ok((child, addr)),
            LaunchOutcome::NotReady(error) if attempt == MAX_SERVER_LAUNCH_ATTEMPTS => {
                return Err(error);
            }
            LaunchOutcome::NotReady(_) => warn!(
                attempt,
                max_attempts = MAX_SERVER_LAUNCH_ATTEMPTS,
                "retrying server launch after readiness failure"
            ),
        }
    }
    Err(anyhow::anyhow!(
        "exhausted server launch attempts without readiness success"
    ))
}

/// Integration test server wrapper that spawns the `mxd` process with the
/// selected backend, waits for readiness, and tears it down automatically on
/// drop.
pub struct TestServer {
    child: Child,
    port: u16,
    bind_addr: SocketAddr,
    db_url: DbUrl,
    /// Held so the test database outlives the server; dropping it drops the
    /// database.
    #[cfg(feature = "postgres")]
    _db: PostgresTestDb,
    temp_dir: Option<TempDir>,
}

impl TestServer {
    /// Launches a server with the default (empty) setup, returning an error if
    /// the database or server cannot be initialized or readiness times out (ten
    /// seconds).
    ///
    /// # Errors
    ///
    /// Returns an error if database or server initialization fails.
    pub fn start(manifest_path: impl Into<ManifestPath>) -> Result<Self, AnyError> {
        Self::start_with_setup(manifest_path, |_| Ok(()))
    }

    /// Launches a server and runs the setup callback with the database URL
    /// before starting, useful for seeding data or running migrations; returns
    /// an error if setup, database initialization, or launch fails.
    ///
    /// # Errors
    ///
    /// Returns an error if setup, database initialization, or launch fails.
    pub fn start_with_setup<F>(
        manifest_path: impl Into<ManifestPath>,
        setup: F,
    ) -> Result<Self, AnyError>
    where
        F: FnOnce(&DbUrl) -> Result<(), AnyError>,
    {
        Self::start_with_binary_and_setup(manifest_path, None, setup)
    }

    /// Launches a server from the prebuilt binary at `binary`, running the
    /// setup callback with the database URL before starting.
    ///
    /// The binary is an argument, so no test changes the process environment to
    /// say which server to run, and concurrent tests do not queue behind a
    /// lock. With `None`, or a `binary` that is not a regular file, the harness
    /// falls back to the process environment and then to `cargo run`, as
    /// `start_with_setup` does.
    ///
    /// # Errors
    ///
    /// Returns an error if setup, database initialization, or launch fails.
    pub fn start_with_binary_and_setup<F>(
        manifest_path: impl Into<ManifestPath>,
        binary: Option<&Path>,
        setup: F,
    ) -> Result<Self, AnyError>
    where
        F: FnOnce(&DbUrl) -> Result<(), AnyError>,
    {
        let target = ServerTarget::new(manifest_path.into(), binary);
        Self::start_target(&target, setup)
    }

    fn start_target<F>(target: &ServerTarget, setup: F) -> Result<Self, AnyError>
    where
        F: FnOnce(&DbUrl) -> Result<(), AnyError>,
    {
        let bind_host = resolve_bind_host()?;
        ensure_single_backend();
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        {
            let temp = TempDir::new()?;
            let db_url = setup_sqlite(&temp, setup)?;
            Self::launch_with(
                target,
                &bind_host,
                db_url,
                move |child, bind_addr, db_url_value| Self {
                    child,
                    port: bind_addr.port(),
                    bind_addr,
                    db_url: db_url_value,
                    temp_dir: Some(temp),
                },
            )
        }

        #[cfg(feature = "postgres")]
        {
            let db = crate::postgres::PostgresTestDb::new()?;
            let db_url = DbUrl::from(db.url.as_ref());
            setup(&db_url)?;
            Self::launch_with(
                target,
                &bind_host,
                db_url,
                move |child, bind_addr, db_url_value| Self {
                    child,
                    port: bind_addr.port(),
                    bind_addr,
                    db_url: db_url_value,
                    _db: db,
                    temp_dir: None,
                },
            )
        }
    }

    fn launch_with<F>(
        target: &ServerTarget,
        bind_host: &str,
        db_url: DbUrl,
        build_self: F,
    ) -> Result<Self, AnyError>
    where
        F: FnOnce(Child, SocketAddr, DbUrl) -> Self,
    {
        let (child, bind_addr) = launch_server_process(target, bind_host, &db_url)?;
        Ok(build_self(child, bind_addr, db_url))
    }

    /// Returns the ephemeral port on which the server is listening.
    #[must_use]
    pub const fn port(&self) -> u16 { self.port }

    /// Returns the database URL used by the server.
    #[must_use]
    pub const fn db_url(&self) -> &DbUrl { &self.db_url }

    /// Returns the bind address used by the server.
    #[must_use]
    pub const fn bind_addr(&self) -> SocketAddr { self.bind_addr }

    /// Returns the temporary directory holding the `SQLite` database, if
    /// applicable. Returns `None` when using `PostgreSQL`.
    #[must_use]
    pub const fn temp_dir(&self) -> Option<&TempDir> { self.temp_dir.as_ref() }
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
