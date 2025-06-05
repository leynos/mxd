use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::unistd::Pid;
use tempfile::TempDir;

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
fn handshake_invalid_protocol() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let db_path = temp.path().join("mxd.db");

    let socket = TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);

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
            db_path.to_str().expect("db path invalid"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut child = ChildGuard(child);

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
                panic!("timeout waiting for server");
            }
        }
    } else {
        panic!("missing stdout from server");
    }

    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let mut handshake = Vec::new();
    handshake.extend_from_slice(b"WRNG");
    handshake.extend_from_slice(&0u32.to_be_bytes());
    handshake.extend_from_slice(&1u16.to_be_bytes());
    handshake.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&handshake)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(&reply[0..4], b"TRTP");
    assert_eq!(u32::from_be_bytes(reply[4..8].try_into().unwrap()), 1);

    #[cfg(unix)]
    kill(Pid::from_raw(child.0.id() as i32), Signal::SIGTERM)?;
    #[cfg(not(unix))]
    child.0.kill()?;
    Ok(())
}
