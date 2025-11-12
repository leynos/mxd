//! Integration test for login validation via the Helix editor.
//!
//! Verifies that a test user can be created and authenticated through the `/server`
//! command with username and password credentials.

use expectrl::{Regex, spawn};
use test_util::{AnyError, TestServer};
use which::which;

#[test]
fn login_validation() -> Result<(), AnyError> {
    if which("hx").is_err() {
        eprintln!("hx not installed; skipping test");
        return Ok(());
    }

    let server = TestServer::start_with_setup("../Cargo.toml", |db| {
        test_util::with_db(db, |conn| {
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

    let port = server.port();
    let mut p = spawn("hx")?;
    p.expect(Regex("HX"))?;
    p.send_line(format!("/server -l test -p secret 127.0.0.1 {}", port))?;
    p.expect(Regex("connected"))?;
    p.send_line("/quit")?;
    Ok(())
}
