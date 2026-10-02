//! Executable configuration contracts, isolated from the parent's environment.

use std::{
    process::{Command, Output, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use cap_std::fs_utf8::Dir;
use rstest::{fixture, rstest};
use tempfile::TempDir;
use wait_timeout::ChildExt;

#[cfg(feature = "sqlite")]
#[path = "configuration_cli/admin.rs"]
mod admin;

struct ServerFixture {
    directory: Dir,
    temporary: TempDir,
}

impl ServerFixture {
    fn run(&self, args: &[&str], environment: &[(&str, &str)]) -> Result<Output> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mxd-wireframe-server"));
        command
            .env_clear()
            .current_dir(self.temporary.path())
            .env("HOME", self.temporary.path())
            .env("XDG_CONFIG_HOME", self.temporary.path())
            .env("XDG_CONFIG_DIRS", self.temporary.path())
            .args(args)
            .envs(environment.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().context("start isolated server")?;
        if child.wait_timeout(Duration::from_secs(20))?.is_none() {
            child.kill()?;
            child.wait()?;
            bail!("configuration command unexpectedly remained running");
        }
        child
            .wait_with_output()
            .context("collect isolated server result")
    }
}

#[fixture]
fn server() -> Result<ServerFixture> {
    let temporary = TempDir::new()?;
    let directory = Dir::open_ambient_dir(
        temporary
            .path()
            .to_str()
            .context("temporary path must be UTF-8")?,
        cap_std::ambient_authority(),
    )?;
    Ok(ServerFixture {
        directory,
        temporary,
    })
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn database_value_equal_to_subcommand_is_preserved(
    server: Result<ServerFixture>,
) -> Result<()> {
    use diesel_async::AsyncConnection;
    use mxd::db::{DbConnection, get_user_by_name};

    let fixture = server?;
    let output = fixture.run(
        &[
            "--database",
            "create-user",
            "--argon2-m-cost",
            "1024",
            "create-user",
            "disposable-user",
            "disposable-password",
        ],
        &[],
    )?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = fixture.temporary.path().join("create-user");
    let mut connection = DbConnection::establish(&path.to_string_lossy()).await?;
    ensure!(
        get_user_by_name(&mut connection, "disposable-user")
            .await?
            .is_some(),
        "configuration contract failed"
    );
    Ok(())
}

#[rstest]
#[case::help("--help")]
#[case::version("--version")]
fn display_requests_skip_configuration_and_storage(
    server: Result<ServerFixture>,
    #[case] flag: &str,
) -> Result<()> {
    let fixture = server?;
    fixture.directory.write(".mxd.toml", "bind = [broken")?;
    let output = fixture.run(
        &["--database", "untouched.db", flag],
        &[("MXD_ARGON2_M_COST", "invalid")],
    )?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let display = String::from_utf8_lossy(&output.stdout);
    if flag == "--help" {
        for option in [
            "--bind",
            "--database",
            "--migration-timeout-secs",
            "--argon2-m-cost",
            "--argon2-t-cost",
            "--argon2-p-cost",
            "--config-path",
            "create-user",
        ] {
            ensure!(
                display.contains(option),
                "missing documented CLI option {option}"
            );
        }
    } else {
        ensure!(
            display.contains(env!("CARGO_PKG_VERSION")),
            "configuration contract failed"
        );
    }
    ensure!(
        !fixture.directory.exists("untouched.db"),
        "configuration contract failed"
    );
    Ok(())
}

#[rstest]
#[case::malformed("bind = [broken")]
#[case::inheritance("extends = 'absent-parent.toml'")]
fn configuration_errors_precede_database_work(
    server: Result<ServerFixture>,
    #[case] contents: &str,
) -> Result<()> {
    let fixture = server?;
    fixture.directory.write(".mxd.toml", contents)?;
    let output = fixture.run(
        &[
            "--database",
            "untouched.db",
            "create-user",
            "disposable-user",
            "disposable-password",
        ],
        &[],
    )?;
    ensure!(!output.status.success(), "configuration contract failed");
    ensure!(
        !fixture.directory.exists("untouched.db"),
        "configuration contract failed"
    );
    Ok(())
}

#[rstest]
#[case::missing_credentials(&["--database", "untouched.db", "create-user"])]
#[case::invalid_memory(&["--database", "untouched.db", "--argon2-m-cost", "1", "create-user", "disposable-user", "disposable-password"])]
#[case::invalid_time(&["--database", "untouched.db", "--argon2-t-cost", "0", "create-user", "disposable-user", "disposable-password"])]
#[case::invalid_parallelism(&["--database", "untouched.db", "--argon2-p-cost", "0", "create-user", "disposable-user", "disposable-password"])]
#[case::missing_password(&["--database", "untouched.db", "create-user", "disposable-user"])]
fn administrative_validation_precedes_database_work(
    server: Result<ServerFixture>,
    #[case] args: &[&str],
) -> Result<()> {
    let fixture = server?;
    let output = fixture.run(args, &[])?;
    ensure!(!output.status.success(), "configuration contract failed");
    ensure!(
        !fixture.directory.exists("untouched.db"),
        "configuration contract failed"
    );
    Ok(())
}
