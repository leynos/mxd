//! Isolated tests for configuration loading at the server CLI boundary.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use argon2::Params;
use cap_std::{ambient_authority, fs::Dir};
use clap::CommandFactory;
use ortho_config::{
    MapEnv,
    OrthoConfig,
    SharedEnvSource,
    SharedScanEnvSource,
    SubcommandFileContext,
    load_and_merge_subcommand_for_with_sources_at,
};
use rstest::rstest;
use tempfile::TempDir;

use super::*;

fn discovery_environment(root: &TempDir) -> MapEnv {
    let xdg_home = root.path().join("xdg-home");
    let xdg_dirs = root.path().join("xdg-dirs");
    MapEnv::new()
        .with_var("HOME", root.path().as_os_str())
        .with_var("XDG_CONFIG_HOME", xdg_home.as_os_str())
        .with_var("XDG_CONFIG_DIRS", xdg_dirs.as_os_str())
}

fn isolated_environment(root: &TempDir) -> Result<MapEnv> {
    let config_path = write_config(root, "empty.toml", "")?;
    Ok(discovery_environment(root).with_var("MXD_CONFIG_PATH", config_path.as_os_str()))
}

fn load_config<I, T>(args: I, environment: MapEnv) -> ortho_config::OrthoResult<AppConfig>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let environment = Arc::new(environment);
    let discovery: SharedEnvSource = environment.clone();
    let merge: SharedScanEnvSource = environment;
    AppConfig::load_from_iter_with_sources(args, discovery, merge)
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

fn write_config(root: &TempDir, name: &str, contents: &str) -> Result<PathBuf> {
    let directory = Dir::open_ambient_dir(root.path(), ambient_authority())
        .context("open test configuration directory")?;
    directory
        .write(name, contents.as_bytes())
        .context("write test configuration")?;
    Ok(root.path().join(name))
}

fn select_file(environment: MapEnv, path: &Path) -> MapEnv {
    environment.with_var("MXD_CONFIG_PATH", path.as_os_str())
}

/// Verifies that our local constants match the upstream argon2 crate defaults.
///
/// This guards against silent drift if argon2 changes its defaults in a
/// future release.
#[rstest]
fn argon2_default_constants_match_upstream() {
    assert_eq!(
        DEFAULT_ARGON2_M_COST,
        Params::DEFAULT_M_COST,
        "DEFAULT_ARGON2_M_COST should match argon2::Params::DEFAULT_M_COST"
    );
    assert_eq!(
        DEFAULT_ARGON2_T_COST,
        Params::DEFAULT_T_COST,
        "DEFAULT_ARGON2_T_COST should match argon2::Params::DEFAULT_T_COST"
    );
    assert_eq!(
        DEFAULT_ARGON2_P_COST,
        Params::DEFAULT_P_COST,
        "DEFAULT_ARGON2_P_COST should match argon2::Params::DEFAULT_P_COST"
    );
}

/// Verifies `AppConfig` uses the expected Argon2 defaults when no overrides
/// are provided.
#[rstest]
fn argon2_defaults_applied_to_config() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let cfg = load_config(["mxd"], isolated_environment(&root)?)?;
    ensure_equal!(
        cfg.argon2_m_cost,
        DEFAULT_ARGON2_M_COST,
        "Argon2 memory cost",
    )?;
    ensure_equal!(cfg.argon2_t_cost, DEFAULT_ARGON2_T_COST, "Argon2 time cost")?;
    ensure_equal!(
        cfg.argon2_p_cost,
        DEFAULT_ARGON2_P_COST,
        "Argon2 parallelism cost",
    )?;
    Ok(())
}

#[rstest]
fn env_config_loading() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let path = write_config(
        &root,
        "config.toml",
        "bind = '1.2.3.4:1111'\ndatabase = 'file.db'",
    )?;
    let environment = select_file(
        isolated_environment(&root)?
            .with_var("MXD_BIND", "127.0.0.1:8000")
            .with_var("MXD_DATABASE", "env.db"),
        &path,
    );
    let cfg = load_config(["mxd"], environment)?;
    ensure_equal!(cfg.bind.as_str(), "127.0.0.1:8000", "bind")?;
    ensure_equal!(cfg.database.as_str(), "env.db", "database")?;
    Ok(())
}

#[rstest]
fn cli_overrides_env() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let environment = isolated_environment(&root)?.with_var("MXD_BIND", "127.0.0.1:8000");
    let cfg = load_config(["mxd", "--bind", "0.0.0.0:9000"], environment)?;
    ensure_equal!(cfg.bind.as_str(), "0.0.0.0:9000", "CLI bind override")?;
    Ok(())
}

#[rstest]
fn explicit_cli_default_overrides_file_and_environment() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let path = write_config(&root, "config.toml", "bind = '1.2.3.4:1111'")?;
    let environment = select_file(
        isolated_environment(&root)?.with_var("MXD_BIND", "127.0.0.1:8000"),
        &path,
    );
    let cfg = load_config(["mxd", "--bind", "0.0.0.0:5500"], environment)?;
    ensure_equal!(cfg.bind.as_str(), "0.0.0.0:5500", "explicit CLI default")?;
    Ok(())
}

#[rstest]
fn migration_timeout_loads_from_env() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let environment = isolated_environment(&root)?.with_var("MXD_MIGRATION_TIMEOUT_SECS", "9");
    let cfg = load_config(["mxd"], environment)?;
    ensure_equal!(cfg.migration_timeout_secs, Some(9), "migration timeout")?;
    Ok(())
}

#[rstest]
fn loads_from_home_dotfile() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    write_config(&root, ".mxd.toml", "bind = '1.2.3.4:1111'")?;
    let cfg = load_config(["mxd"], discovery_environment(&root))?;
    ensure_equal!(cfg.bind.as_str(), "1.2.3.4:1111", "home dotfile bind")?;
    Ok(())
}

#[rstest]
fn argon2_cli_overrides_all_params() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let cfg = load_config(
        [
            "mxd",
            "--argon2-m-cost",
            "1024",
            "--argon2-t-cost",
            "4",
            "--argon2-p-cost",
            "2",
        ],
        isolated_environment(&root)?,
    )?;
    ensure_equal!(cfg.argon2_m_cost, 1024, "CLI Argon2 memory cost")?;
    ensure_equal!(cfg.argon2_t_cost, 4, "CLI Argon2 time cost")?;
    ensure_equal!(cfg.argon2_p_cost, 2, "CLI Argon2 parallelism cost")?;
    Ok(())
}

#[rstest]
fn argon2_env_overrides() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let environment = isolated_environment(&root)?
        .with_var("MXD_ARGON2_M_COST", "2048")
        .with_var("MXD_ARGON2_T_COST", "8")
        .with_var("MXD_ARGON2_P_COST", "4");
    let cfg = load_config(["mxd"], environment)?;
    ensure_equal!(cfg.argon2_m_cost, 2048, "environment Argon2 memory cost")?;
    ensure_equal!(cfg.argon2_t_cost, 8, "environment Argon2 time cost")?;
    ensure_equal!(cfg.argon2_p_cost, 4, "environment Argon2 parallelism cost")?;
    Ok(())
}

#[rstest]
fn argon2_config_file_overrides() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let path = write_config(
        &root,
        "config.toml",
        concat!(
            "argon2_m_cost = 4096\n",
            "argon2_t_cost = 16\n",
            "argon2_p_cost = 8\n"
        ),
    )?;
    let environment = select_file(isolated_environment(&root)?, &path);
    let cfg = load_config(["mxd"], environment)?;
    ensure_equal!(cfg.argon2_m_cost, 4096, "file Argon2 memory cost")?;
    ensure_equal!(cfg.argon2_t_cost, 16, "file Argon2 time cost")?;
    ensure_equal!(cfg.argon2_p_cost, 8, "file Argon2 parallelism cost")?;
    Ok(())
}

#[rstest]
fn argon2_cli_overrides_env_and_file() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let path = write_config(&root, "config.toml", "argon2_m_cost = 4096\n")?;
    let environment = select_file(
        isolated_environment(&root)?.with_var("MXD_ARGON2_M_COST", "2048"),
        &path,
    );
    let cfg = load_config(["mxd", "--argon2-m-cost", "1024"], environment)?;
    ensure_equal!(cfg.argon2_m_cost, 1024, "CLI Argon2 precedence")?;
    Ok(())
}

#[rstest]
fn subcommand_merge_preserves_file_environment_and_cli_precedence() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let command_name = "create-user";
    let command = CreateUserArgs::command();
    ensure_equal!(command.get_name(), command_name, "subcommand name",)?;
    let contents = format!(
        "[cmds.{command_name}]\nusername = 'file-user'\n[cmds.other-command]\nusername = \
         'other-user'\n"
    );
    write_config(&root, ".mxd.toml", &contents)?;

    let discovery = MapEnv::new()
        .with_var("HOME", root.path().as_os_str())
        .with_var("XDG_CONFIG_HOME", root.path().join("xdg-home").as_os_str())
        .with_var("XDG_CONFIG_DIRS", root.path().join("xdg-dirs").as_os_str());
    let files = SubcommandFileContext::new(root.path(), &discovery);
    let env_key = format!(
        "MXD_CMDS_{}_USERNAME",
        command_name.replace('-', "_").to_ascii_uppercase()
    );

    let from_file = load_and_merge_subcommand_for_with_sources_at::<CreateUserArgs>(
        &CreateUserArgs::default(),
        files,
        Arc::new(MapEnv::new()),
    )?;
    ensure_equal!(
        from_file.username.as_deref(),
        Some("file-user"),
        "subcommand file username",
    )?;

    let from_environment = load_and_merge_subcommand_for_with_sources_at::<CreateUserArgs>(
        &CreateUserArgs::default(),
        files,
        Arc::new(MapEnv::new().with_var(&env_key, "environment-user")),
    )?;
    ensure_equal!(
        from_environment.username.as_deref(),
        Some("environment-user"),
        "subcommand environment username",
    )?;

    let cli = CreateUserArgs::try_parse_from([command_name, "cli-user"])
        .context("parse create-user arguments")?;
    let from_cli = load_and_merge_subcommand_for_with_sources_at::<CreateUserArgs>(
        &cli,
        files,
        Arc::new(MapEnv::new().with_var(&env_key, "environment-user")),
    )?;
    ensure_equal!(
        from_cli.username.as_deref(),
        Some("cli-user"),
        "subcommand CLI username",
    )?;
    Ok(())
}

#[cfg(any(feature = "json5", feature = "yaml"))]
#[path = "cli_tests/formats.rs"]
mod formats;

#[path = "cli_tests/file_errors.rs"]
mod file_errors;

#[path = "cli_tests/precedence.rs"]
mod precedence;

#[cfg(not(all(feature = "json5", feature = "yaml")))]
#[path = "cli_tests/disabled_formats.rs"]
mod disabled_formats;
