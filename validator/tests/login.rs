use expectrl::{spawn, Regex};
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use tempfile::TempDir;
use which::which;

/// Kill the child process when dropped to avoid orphans if the test panics.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn login_validation() -> Result<(), Box<dyn std::error::Error>> {
    if which("shx").is_err() {
        eprintln!("shx not installed; skipping test");
        return Ok(());
    }
    let temp = TempDir::new()?;
    let db_path = temp.path().join("mxd.db");

    // create user
    let status = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            "../Cargo.toml",
            "--quiet",
            "--",
            "--database",
            db_path
                .to_str()
                .expect("Database path is not valid UTF-8"),
            "create-user",
            "test",
            "secret",
        ])
        .status()?;
    assert!(status.success());

    // pick a free port for the server
    let socket = TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);

    // start server
    let child = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            "../Cargo.toml",
            "--quiet",
            "--",
            "--bind",
            &format!("127.0.0.1:{}", port),
            "--database",
            db_path
                .to_str()
                .expect("Database path is not valid UTF-8"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut child = ChildGuard(child);

    // wait for server to start by reading stdout with a timeout
    if let Some(out) = &mut child.0.stdout {
        let mut reader = BufReader::new(out);
        let mut line = String::new();
        let timeout = Duration::from_secs(10);
        let start = Instant::now();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                panic!("server exited before signaling readiness");
            }
            if line.contains("listening on") {
                break;
            }
            if start.elapsed() > timeout {
                panic!("timeout waiting for server to signal readiness");
            }
        }
    } else {
        panic!("missing stdout from server");
    }

    // spawn shx client using expect
    let mut p = spawn(&format!("shx 127.0.0.1 {}", port))?;
    p.expect(Regex("MXD"))?;
    p.send_line("LOGIN test secret")?;
    p.expect(Regex("OK"))?;
    p.send_line("/quit")?;
    p.expect(Regex("OK"))?;

    child.0.kill()?;
    Ok(())
}
