//! Integration test for login validation via the Helix editor.
//!
//! Verifies that a test user can be created and authenticated through the `/server`
//! command with username and password credentials.

use expectrl::{Regex, spawn};
use test_util::{AnyError, DatabaseUrl, TestServer};
use which::which;

#[test]
fn login_validation() -> Result<(), AnyError> {
    if which("hx").is_err() {
        #[expect(
            clippy::print_stderr,
            reason = "skip message: inform user why test is being skipped"
        )]
        {
            eprintln!("hx not installed; skipping test");
        }
        return Ok(());
    }

    let server = TestServer::start_with_setup("../Cargo.toml", |db| {
        test_util::with_db(DatabaseUrl::new(db.as_str()), |conn| {
            Box::pin(async move {
                use argon2::Argon2;
                use mxd::{db::create_user, models::NewUser, users::hash_password};

                let argon2 = Argon2::default();
                let hashed = hash_password(&argon2, "secret")?;
                let new_user = NewUser {
                    username: "test",
                    password: &hashed,
                };
                create_user(conn, &new_user).await?;
                Ok(())
            })
        })
    })?;

    let bind_addr = server.bind_addr();
    let host = bind_addr.ip();
    let port = bind_addr.port();
    let mut p = spawn("hx")?;
    p.expect(Regex("HX"))?;
    p.send_line(format!("/server -l test -p secret {host} {port}"))?;
    p.expect(Regex("connected"))?;
    p.send_line("/quit")?;
    Ok(())
}
