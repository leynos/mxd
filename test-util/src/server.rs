use std::{
    ffi::OsString,
    io::{BufRead, BufReader},
    net::TcpListener,
    process::{Child, Command, Stdio},
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

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("Either feature 'sqlite' or 'postgres' must be enabled");

#[inline]
fn ensure_single_backend() {
    assert!(
        !cfg!(all(feature = "sqlite", feature = "postgres")),
        "Choose either sqlite or postgres, not both",
    );
}

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
    db_url: String,
    #[cfg(feature = "postgres")]
    db: PostgresTestDb,
    temp_dir: Option<TempDir>,
}

impl TestServer {
    pub fn start(manifest_path: &str) -> Result<Self, AnyError> {
        Self::start_with_setup(manifest_path, |_| Ok(()))
    }

    pub fn start_with_setup<F>(manifest_path: &str, setup: F) -> Result<Self, AnyError>
    where
        F: FnOnce(&str) -> Result<(), AnyError>,
    {
        ensure_single_backend();
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        {
            let temp = TempDir::new()?;
            let db_url = setup_sqlite(&temp, setup)?;
            Self::launch(manifest_path, db_url, Some(temp))
        }

        #[cfg(feature = "postgres")]
        {
            let db = crate::postgres::PostgresTestDb::new()?;
            setup(db.url.as_ref())?;
            Self::launch(manifest_path, db)
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

    pub fn port(&self) -> u16 { self.port }

    pub fn db_url(&self) -> &str { self.db_url.as_str() }

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
