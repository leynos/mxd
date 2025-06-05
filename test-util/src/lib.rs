use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use std::path::{Path, PathBuf};
use tempfile::TempDir;

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
    db_path: PathBuf,
    _temp: TempDir,
}

impl TestServer {
    /// Start the server using the given Cargo manifest path.
    pub fn start(manifest_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_setup(manifest_path, |_| Ok(()))
    }

    /// Start the server and run a setup function before launching.
    pub fn start_with_setup<F>(
        manifest_path: &str,
        setup: F,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: FnOnce(&Path) -> Result<(), Box<dyn std::error::Error>>,
    {
        let temp = TempDir::new()?;
        let db_path = temp.path().join("mxd.db");

        setup(&db_path)?;

        let socket = TcpListener::bind("127.0.0.1:0")?;
        let port = socket.local_addr()?.port();
        drop(socket);

        let mut child = Command::new("cargo")
            .args([
                "run",
                "--manifest-path",
                manifest_path,
                "--quiet",
                "--",
                "--bind",
                &format!("127.0.0.1:{}", port),
                "--database",
                db_path.to_str().expect("database path utf8"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

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
        } else {
            return Err("missing stdout from server".into());
        }

        Ok(Self {
            child,
            port,
            db_path,
            _temp: temp,
        })
    }

    /// Return the port the server is bound to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Path to the SQLite database used by this server.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
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
