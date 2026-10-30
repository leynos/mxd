//! Command-line interface definitions for the MXD server.
//!
//! This module re-exports CLI types from the `cli-defs` crate, which provides
//! stable definitions shared between build-time (man page generation) and
//! runtime consumers.

use std::ffi::{OsStr, OsString};

use anyhow::{Context, Result};
use argon2::Params;
use clap::{CommandFactory, Parser};
pub use cli_defs::{
    AppConfig,
    Cli,
    Commands,
    CreateUserArgs,
    DEFAULT_ARGON2_M_COST,
    DEFAULT_ARGON2_P_COST,
    DEFAULT_ARGON2_T_COST,
};
use ortho_config::OrthoConfig;

const _: () = {
    assert!(DEFAULT_ARGON2_M_COST == Params::DEFAULT_M_COST);
    assert!(DEFAULT_ARGON2_T_COST == Params::DEFAULT_T_COST);
    assert!(DEFAULT_ARGON2_P_COST == Params::DEFAULT_P_COST);
};

/// Parsed CLI with resolved configuration values.
#[derive(Debug, Clone)]
pub struct ResolvedCli {
    /// Application configuration.
    pub config: AppConfig,
    /// Optional subcommand.
    pub command: Option<Commands>,
}

/// Load configuration using `OrthoConfig` defaults and CLI overrides.
///
/// # Errors
///
/// Returns an error if configuration parsing fails.
pub fn load_cli() -> Result<ResolvedCli> {
    let cli = Cli::parse();
    let config_args = config_args_without_subcommand();
    let config = AppConfig::load_from_iter(config_args).context("load configuration")?;
    Ok(ResolvedCli {
        config,
        command: cli.command,
    })
}

/// Collect program arguments with any subcommand and its trailing args stripped.
///
/// Scans arguments and stops at the `--` separator to avoid treating literal
/// values as subcommands. A candidate command is a boundary only when clap
/// accepts the preceding arguments, so a value that happens to equal a command
/// name remains attached to its option. `OrthoConfig`'s parser then receives
/// only the arguments preceding the actual subcommand.
fn config_args_without_subcommand() -> Vec<OsString> {
    config_args_from(&std::env::args_os().collect::<Vec<_>>())
}

/// Return the arguments that precede a subcommand boundary.
///
/// `args` is a complete argument vector whose first element is the program
/// name, the shape `std::env::args_os` produces. The scan is pure so tests can
/// drive it with generated vectors instead of a fabricated process.
///
/// # Examples
///
/// Given `["mxd", "--database", "db", "create-user", "alice"]` the scan stops
/// at `create-user` and returns `["mxd", "--database", "db"]`.
fn config_args_from(args: &[OsString]) -> Vec<OsString> {
    // OrthoConfig's CLI parser does not handle subcommands, so strip any
    // subcommand and its arguments before loading configuration.
    let end = subcommand_boundary(args).unwrap_or(args.len());
    args.iter().take(end).cloned().collect()
}

/// Find the index of the token that starts the subcommand section.
///
/// A token equal to a subcommand name is a boundary only when clap accepts
/// every preceding token, so a value that happens to equal a command name
/// stays attached to its option. The `--` separator ends the scan.
///
/// # Examples
///
/// For `["mxd", "--database", "create-user"]`, `create-user` is the value of
/// `--database`, the preceding tokens do not parse, and no boundary is
/// reported.
fn subcommand_boundary(args: &[OsString]) -> Option<usize> {
    let subcommands = subcommand_names();
    args.iter()
        .enumerate()
        .skip(1)
        .take_while(|(_, arg)| arg.as_os_str() != OsStr::new("--"))
        .find(|(index, arg)| {
            is_subcommand_name(arg, &subcommands)
                && Cli::try_parse_from(args.iter().take(*index).cloned()).is_ok()
        })
        .map(|(index, _)| index)
}

/// Names of the CLI's subcommands, spelled as clap spells them.
fn subcommand_names() -> Vec<String> {
    Cli::command()
        .get_subcommands()
        .map(|cmd| cmd.get_name().to_owned())
        .collect()
}

/// Whether `arg` spells one of `subcommands` exactly.
fn is_subcommand_name(arg: &OsStr, subcommands: &[String]) -> bool {
    arg.to_str()
        .is_some_and(|value| subcommands.iter().any(|name| name == value))
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
