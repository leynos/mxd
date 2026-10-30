//! Verify credential precedence through the real administrative command.

#![cfg(all(feature = "sqlite", feature = "toml"))]

use std::{process::Command, sync::Arc};

use anyhow::{Context, Result, anyhow};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use cap_std::{ambient_authority, fs::Dir};
use clap::Parser;
use diesel_async::AsyncConnection;
use mxd::{
    db::{DbConnection, get_user_by_name},
    server::CreateUserArgs,
};
use ortho_config::{MapEnv, SubcommandFileContext, load_and_merge_subcommand_for_with_sources_at};
use rstest::rstest;
use tempfile::TempDir;

const FILE_PASSWORD: &str = "file-password";
const ENV_PASSWORD: &str = "environment-password";
const CLI_PASSWORD: &str = "command-line-password";
const USERNAME: &str = "credential-precedence-user";
const PASSWORD_ENV: &str = "MXD_CMDS_CREATE_USER_PASSWORD";

fn write_config(root: &TempDir) -> Result<()> {
    let directory = Dir::open_ambient_dir(root.path(), ambient_authority())
        .context("open isolated credential fixture")?;
    for name in ["home", "xdg-home", "xdg-dirs"] {
        directory
            .create_dir(name)
            .with_context(|| format!("create isolated {name} directory"))?;
    }
    directory
        .write(
            ".mxd.toml",
            format!("[cmds.create-user]\npassword = '{FILE_PASSWORD}'\n"),
        )
        .context("write credential configuration")?;
    Ok(())
}

fn discovery_environment(root: &TempDir) -> MapEnv {
    MapEnv::new()
        .with_var("HOME", root.path().join("home").as_os_str())
        .with_var("XDG_CONFIG_HOME", root.path().join("xdg-home").as_os_str())
        .with_var("XDG_CONFIG_DIRS", root.path().join("xdg-dirs").as_os_str())
}

fn verify_lower_priority_credentials(root: &TempDir) -> Result<()> {
    let discovery = discovery_environment(root);
    let files = SubcommandFileContext::new(root.path(), &discovery);
    let from_file = load_and_merge_subcommand_for_with_sources_at::<CreateUserArgs>(
        &CreateUserArgs::default(),
        files,
        Arc::new(MapEnv::new()),
    )?;
    anyhow::ensure!(
        from_file.password.as_deref() == Some(FILE_PASSWORD),
        "file must supply the lower-priority password"
    );

    let from_environment = load_and_merge_subcommand_for_with_sources_at::<CreateUserArgs>(
        &CreateUserArgs::default(),
        files,
        Arc::new(MapEnv::new().with_var(PASSWORD_ENV, ENV_PASSWORD)),
    )?;
    anyhow::ensure!(
        from_environment.password.as_deref() == Some(ENV_PASSWORD),
        "environment must override the file password"
    );
    Ok(())
}

#[rstest]
#[tokio::test]
async fn cli_password_is_restored_after_merge_and_used_for_the_hash() -> Result<()> {
    let root = TempDir::new().context("create isolated credential fixture")?;
    write_config(&root)?;
    verify_lower_priority_credentials(&root)?;

    let cli = CreateUserArgs::try_parse_from(["create-user", USERNAME, CLI_PASSWORD])
        .context("parse explicit credentials")?;
    let serialized = serde_json::to_value(&cli).context("serialize create-user arguments")?;
    anyhow::ensure!(
        serialized.get("password").is_none(),
        "password must remain excluded from serialization"
    );

    let database_path = root.path().join("credentials.db");
    let database = database_path
        .to_str()
        .context("database path must be UTF-8")?;
    let output = Command::new(env!("CARGO_BIN_EXE_mxd-wireframe-server"))
        .env_clear()
        .env("HOME", root.path().join("home"))
        .env("XDG_CONFIG_HOME", root.path().join("xdg-home"))
        .env("XDG_CONFIG_DIRS", root.path().join("xdg-dirs"))
        .env(PASSWORD_ENV, ENV_PASSWORD)
        .current_dir(root.path())
        .args([
            "--database",
            database,
            "create-user",
            USERNAME,
            CLI_PASSWORD,
        ])
        .output()
        .context("run create-user with conflicting passwords")?;
    anyhow::ensure!(
        output.status.success(),
        "create-user failed with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let mut connection = DbConnection::establish(database)
        .await
        .context("open disposable credentials database")?;
    let user = get_user_by_name(&mut connection, USERNAME)
        .await?
        .ok_or_else(|| anyhow!("created user was not persisted"))?;
    let hash = PasswordHash::new(&user.password)
        .map_err(|error| anyhow!("parse persisted Argon2 hash: {error}"))?;
    let verifier = Argon2::default();
    anyhow::ensure!(
        verifier
            .verify_password(CLI_PASSWORD.as_bytes(), &hash)
            .is_ok(),
        "stored hash must verify the explicit CLI password"
    );
    for lower_priority_password in [ENV_PASSWORD, FILE_PASSWORD] {
        anyhow::ensure!(
            verifier
                .verify_password(lower_priority_password.as_bytes(), &hash)
                .is_err(),
            "stored hash must not verify a lower-priority password"
        );
    }
    Ok(())
}
