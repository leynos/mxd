use expectrl::{spawn, Regex};
use std::process::{Command, Stdio};
use tempfile::TempDir;
use std::thread;
use std::time::Duration;
use which::which;

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
            db_path.to_str().unwrap(),
            "create-user",
            "test",
            "secret",
        ])
        .status()?;
    assert!(status.success());

    // start server
    let mut child = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            "../Cargo.toml",
            "--quiet",
            "--",
            "--bind",
            "127.0.0.1:5599",
            "--database",
            db_path.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    // wait for server to start
    thread::sleep(Duration::from_secs(1));

    // spawn shx client using expect
    let mut p = spawn("shx 127.0.0.1 5599")?;
    p.expect(Regex("MXD"))?;
    p.send_line("LOGIN test secret")?;
    p.expect(Regex("OK"))?;
    p.send_line("/quit")?;
    p.expect(Regex("OK"))?;

    child.kill()?;
    Ok(())
}
