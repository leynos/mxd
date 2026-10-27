//! Process-level checks for mxd CLI output and exit contracts.

use std::{path::Path, process::Command};

use anyhow::{Context, Result};
use cap_std::{ambient_authority, fs::Dir};
use rstest::rstest;
use tempfile::TempDir;

const GENERATED_MAN_PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/mxd.1"));

fn run_mxd(
    args: &[&str],
    environment: &[(&str, &str)],
    current_dir: Option<&Path>,
) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mxd"));
    command
        .env_clear()
        .env("LC_ALL", "C")
        .args(args)
        .envs(environment.iter().copied());
    if let Some(directory) = current_dir {
        command.current_dir(directory);
    }
    command.output()
}

fn write_fixture(root: &TempDir, name: &str, contents: &str) -> Result<()> {
    let directory = Dir::open_ambient_dir(root.path(), ambient_authority())
        .context("open CLI fixture directory")?;
    directory
        .write(name, contents.as_bytes())
        .context("write CLI fixture")
}

macro_rules! ensure_equal {
    ($actual:expr, $expected:expr, $context:expr $(,)?) => {{
        let actual = $actual;
        let expected = $expected;
        anyhow::ensure!(
            actual == expected,
            "{}: expected {expected:?}, got {actual:?}",
            $context
        );
        Ok::<(), anyhow::Error>(())
    }};
}

#[rstest]
fn help_uses_stdout_and_exits_successfully() -> Result<()> {
    let output = run_mxd(&["--help"], &[], None)?;
    ensure_equal!(output.status.code(), Some(0), "help exit status")?;
    anyhow::ensure!(output.stdout.windows(5).any(|w| w == b"Usage"));
    anyhow::ensure!(output.stdout.windows(4).any(|w| w == b"bind"));
    anyhow::ensure!(output.stderr.is_empty());
    Ok(())
}

#[rstest]
fn unknown_subcommand_uses_stderr_and_exits_with_clap_status() -> Result<()> {
    let output = run_mxd(&["not-a-command"], &[], None)?;
    ensure_equal!(
        output.status.code(),
        Some(2),
        "unknown subcommand exit status",
    )?;
    anyhow::ensure!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::ensure!(stderr.contains("unrecognized subcommand"), "{stderr}");
    Ok(())
}

#[rstest]
fn clap_value_error_uses_stderr_and_exits_with_clap_status() -> Result<()> {
    let output = run_mxd(&["--argon2-m-cost", "not-a-number"], &[], None)?;
    ensure_equal!(output.status.code(), Some(2), "invalid value exit status")?;
    anyhow::ensure!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::ensure!(stderr.contains("invalid value"), "{stderr}");
    Ok(())
}

#[rstest]
fn missing_explicit_config_fails_visibly() -> Result<()> {
    let root = TempDir::new().context("create missing-config fixture root")?;
    let missing = root.path().join("missing.toml");
    let missing_path = missing
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("temporary path is not UTF-8"))?;
    let output = run_mxd(
        &["--bind", "127.0.0.1:5500"],
        &[("MXD_CONFIG_PATH", missing_path)],
        None,
    )?;
    ensure_equal!(
        output.status.code(),
        Some(1),
        "missing environment config status",
    )?;
    anyhow::ensure!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::ensure!(stderr.contains("load configuration"), "{stderr}");
    anyhow::ensure!(stderr.contains("missing.toml"), "{stderr}");
    Ok(())
}

#[rstest]
fn missing_cli_selected_config_fails_visibly() -> Result<()> {
    let root = TempDir::new().context("create missing-config fixture root")?;
    let missing_path = root
        .path()
        .join("missing.toml")
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("temporary path is not UTF-8"))?
        .to_owned();
    let output = run_mxd(
        &["--config-path", &missing_path, "--bind", "127.0.0.1:5500"],
        &[],
        None,
    )?;
    ensure_equal!(output.status.code(), Some(1), "missing CLI config status")?;
    anyhow::ensure!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::ensure!(stderr.contains("load configuration"), "{stderr}");
    anyhow::ensure!(stderr.contains("missing.toml"), "{stderr}");
    Ok(())
}

#[rstest]
fn malformed_explicit_config_fails_visibly() -> Result<()> {
    let root = TempDir::new().context("create malformed-config fixture root")?;
    write_fixture(&root, "malformed.toml", "bind = [")?;
    let path = root
        .path()
        .join("malformed.toml")
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("temporary path is not UTF-8"))?
        .to_owned();
    let output = run_mxd(
        &["--bind", "127.0.0.1:5500"],
        &[("MXD_CONFIG_PATH", &path)],
        None,
    )?;
    ensure_equal!(output.status.code(), Some(1), "malformed config status")?;
    anyhow::ensure!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::ensure!(stderr.contains("load configuration"), "{stderr}");
    anyhow::ensure!(stderr.contains("malformed.toml"), "{stderr}");
    Ok(())
}

#[rstest]
fn local_dotfile_discovery_surfaces_parse_failures() -> Result<()> {
    let root = TempDir::new().context("create dotfile fixture root")?;
    let directory = Dir::open_ambient_dir(root.path(), ambient_authority())
        .context("open isolated discovery root")?;
    for path in ["home", "xdg-config-home", "xdg-config-dirs"] {
        directory
            .create_dir(path)
            .with_context(|| format!("create isolated discovery directory {path}"))?;
    }
    let home = root.path().join("home");
    let xdg_config_home = root.path().join("xdg-config-home");
    let xdg_config_dirs = root.path().join("xdg-config-dirs");
    let environment = [
        (
            "HOME",
            home.to_str()
                .ok_or_else(|| anyhow::anyhow!("temporary HOME path is not UTF-8"))?,
        ),
        (
            "XDG_CONFIG_HOME",
            xdg_config_home
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("temporary XDG_CONFIG_HOME is not UTF-8"))?,
        ),
        (
            "XDG_CONFIG_DIRS",
            xdg_config_dirs
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("temporary XDG_CONFIG_DIRS is not UTF-8"))?,
        ),
    ];
    write_fixture(&root, ".mxd.toml", "bind = [")?;
    let output = run_mxd(
        &["--bind", "127.0.0.1:5500"],
        &environment,
        Some(root.path()),
    )?;
    ensure_equal!(
        output.status.code(),
        Some(1),
        "discovered malformed config status",
    )?;
    anyhow::ensure!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::ensure!(stderr.contains("load configuration"), "{stderr}");
    anyhow::ensure!(stderr.contains(".mxd.toml"), "{stderr}");
    Ok(())
}

#[rstest]
fn generated_man_page_keeps_shared_cli_options_and_subcommands() {
    assert!(GENERATED_MAN_PAGE.contains(r"\-\-bind"));
    assert!(GENERATED_MAN_PAGE.contains(r"create\-user"));
}
