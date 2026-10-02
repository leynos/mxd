//! Administrative namespace and credential priority through disposable `SQLite` storage.

use anyhow::{Context, Result, ensure};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use diesel_async::AsyncConnection;
use mxd::db::{DbConnection, get_user_by_name};
use rstest::rstest;

use super::{ServerFixture, server};

#[rstest]
#[case::file(&[], &[], ("file-user", "file-password"))]
#[case::environment(&[], &[("MXD_CMDS_CLI_DEFS_USERNAME", "env-user"), ("MXD_CMDS_CLI_DEFS_PASSWORD", "env-password")], ("env-user", "env-password"))]
#[case::cli(&["cli-user", "cli-password"], &[("MXD_CMDS_CLI_DEFS_USERNAME", "env-user"), ("MXD_CMDS_CLI_DEFS_PASSWORD", "env-password")], ("cli-user", "cli-password"))]
#[tokio::test]
async fn administrative_credentials_follow_file_environment_cli_priority(
    server: Result<ServerFixture>,
    #[case] credentials: &[&str],
    #[case] environment: &[(&str, &str)],
    #[case] expected: (&str, &str),
) -> Result<()> {
    let (username, password) = expected;
    let fixture = server?;
    fixture.directory.write(
        ".mxd.toml",
        concat!(
            "database = 'disposable.db'\nargon2_m_cost = 1024\n",
            "[cmds.cli-defs]\nusername = 'file-user'\npassword = 'file-password'\n",
        ),
    )?;
    let mut args = vec!["create-user"];
    args.extend_from_slice(credentials);
    let output = fixture.run(&args, environment)?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Neither diagnostics nor the successful command's output may expose the secret.
    ensure!(
        !String::from_utf8_lossy(&output.stdout).contains(password),
        "configuration contract failed"
    );
    ensure!(
        !String::from_utf8_lossy(&output.stderr).contains(password),
        "configuration contract failed"
    );
    verify_stored_credentials(&fixture, username, password).await
}

async fn verify_stored_credentials(
    fixture: &ServerFixture,
    username: &str,
    password: &str,
) -> Result<()> {
    let path = fixture.temporary.path().join("disposable.db");
    let mut connection = DbConnection::establish(&path.to_string_lossy()).await?;
    let user = get_user_by_name(&mut connection, username)
        .await?
        .context("selected namespace creates the expected user")?;
    let hash = PasswordHash::new(&user.password)
        .map_err(|error| anyhow::anyhow!("parse hash: {error}"))?;
    ensure!(
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok(),
        "configuration contract failed"
    );
    if password != "file-password" {
        ensure!(
            Argon2::default()
                .verify_password(b"file-password", &hash)
                .is_err(),
            "configuration contract failed"
        );
    }
    Ok(())
}

#[rstest]
#[case::toml(".mxd.toml", include_str!("fixtures/admin.toml"))]
#[cfg_attr(feature = "json5", case::json5(".mxd.json5", include_str!("fixtures/admin.json5")))]
#[cfg_attr(feature = "yaml", case::yaml(".mxd.yaml", include_str!("fixtures/admin.yaml")))]
#[tokio::test]
async fn selected_file_formats_reach_both_runtime_and_administrative_consumers(
    server: Result<ServerFixture>,
    #[case] filename: &str,
    #[case] contents: &str,
) -> Result<()> {
    let fixture = server?;
    fixture.directory.write(filename, contents)?;
    // Server selection and conventional subcommand discovery reach the same file.
    let output = fixture.run(&["--config-path", filename, "create-user"], &[])?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    verify_stored_credentials(&fixture, "format-user", "format-password").await
}
