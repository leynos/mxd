//! Command-line interface definitions for the MXD server.
//!
//! This module re-exports CLI types from the `cli-defs` crate, which provides
//! stable definitions shared between build-time (man page generation) and
//! runtime consumers.

use anyhow::{Context, Result};
use argon2::Params;
use clap::Parser;
pub use cli_defs::{
    AppConfig,
    Cli,
    Commands,
    CreateUserArgs,
    DEFAULT_ARGON2_M_COST,
    DEFAULT_ARGON2_P_COST,
    DEFAULT_ARGON2_T_COST,
};

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
    let config = cli.resolve_config().context("load configuration")?;
    Ok(ResolvedCli {
        config,
        command: cli.command,
    })
}

#[cfg(test)]
mod tests {
    //! Tests for this module.
    use argon2::Params;
    use rstest::rstest;

    use super::*;

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
}
