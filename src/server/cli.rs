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
    let args: Vec<OsString> = std::env::args_os().collect();
    let subcommands = Cli::command()
        .get_subcommands()
        .map(|cmd| cmd.get_name().to_owned())
        .collect::<Vec<_>>();
    let mut subcommand_index = None;
    for (index, arg) in args.iter().enumerate().skip(1) {
        if arg.as_os_str() == OsStr::new("--") {
            break;
        }
        if arg
            .to_str()
            .is_some_and(|value| subcommands.iter().any(|name| name == value))
            && Cli::try_parse_from(args.iter().take(index).cloned()).is_ok()
        {
            subcommand_index = Some(index);
            break;
        }
    }
    // OrthoConfig's CLI parser does not handle subcommands, so strip any
    // subcommand and its arguments before loading configuration.
    if let Some(index) = subcommand_index {
        args.into_iter().take(index).collect()
    } else {
        args
    }
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
