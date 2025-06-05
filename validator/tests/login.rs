use expectrl::{spawn, Regex};
use test_util::TestServer;
use which::which;

#[test]
fn login_validation() -> Result<(), Box<dyn std::error::Error>> {
    if which("shx").is_err() {
        eprintln!("shx not installed; skipping test");
        return Ok(());
    }

    let server = TestServer::start_with_setup("../Cargo.toml", |db| {
        let status = std::process::Command::new("cargo")
            .args([
                "run",
                "--manifest-path",
                "../Cargo.toml",
                "--quiet",
                "--",
                "--database",
                db.to_str().expect("db path utf8"),
                "create-user",
                "test",
                "secret",
            ])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err("failed to create user".into())
        }
    })?;

    let port = server.port();
    let mut p = spawn(&format!("shx 127.0.0.1 {}", port))?;
    p.expect(Regex("MXD"))?;
    p.send_line("LOGIN test secret")?;
    p.expect(Regex("OK"))?;
    p.send_line("/quit")?;
    p.expect(Regex("OK"))?;
    Ok(())
}
