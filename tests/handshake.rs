use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::unistd::Pid;
use tempfile::TempDir;

/// Kill the child process when dropped to avoid orphans if the test panics.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = kill(Pid::from_raw(self.0.id() as i32), Signal::SIGTERM);
        }
        #[cfg(not(unix))]
        {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn handshake() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let db_path = temp.path().join("mxd.db");

    // pick a free port for the server
    let socket = TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);

    // start server
    let child = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            "./Cargo.toml",
            "--quiet",
            "--",
            "--bind",
            &format!("127.0.0.1:{}", port),
            "--database",
            db_path.to_str().expect("Database path is not valid UTF-8"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut child = ChildGuard(child);

    // wait for server to start
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

    // connect to server directly
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let mut handshake = Vec::new();
    handshake.extend_from_slice(b"TRTP");
    handshake.extend_from_slice(&0u32.to_be_bytes());
    handshake.extend_from_slice(&1u16.to_be_bytes());
    handshake.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&handshake)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(&reply[0..4], b"TRTP");
    assert_eq!(u32::from_be_bytes(reply[4..8].try_into().unwrap()), 0);

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    assert_eq!(line.trim_end(), "MXD");

    #[cfg(unix)]
    kill(Pid::from_raw(child.0.id() as i32), Signal::SIGTERM)?;
    #[cfg(not(unix))]
    child.0.kill()?;
    Ok(())
}
