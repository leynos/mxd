//! Shared CLI type definitions for mxd build and runtime.
//!
//! This crate provides CLI argument and configuration types used by both the
//! `build.rs` script (for man page generation) and the runtime binaries.
//! Extracting these types into a separate crate avoids brittle `#[path = ...]`
//! includes and keeps build-time and runtime dependencies cleanly separated.

use std::{borrow::Cow, ffi::OsString};

use clap::{Args, Parser, Subcommand};
use ortho_config::{
    MergeLayer,
    OrthoConfig,
    OrthoResult,
    declarative::LayerComposition,
    sanitize_value,
};
use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────────────────
// Argon2 default parameters
//
// These constants duplicate `argon2::Params::DEFAULT_*` values so that
// build-time consumers (man page generation) can use this crate without
// adding `argon2` as a build-dependency.
//
// Values as of argon2 0.5.x:
//   DEFAULT_M_COST = 19_456
//   DEFAULT_T_COST = 2
//   DEFAULT_P_COST = 1
// ────────────────────────────────────────────────────────────────────────────

/// Default Argon2 memory cost (matches `argon2::Params::DEFAULT_M_COST`).
pub const DEFAULT_ARGON2_M_COST: u32 = 19_456;
/// Default Argon2 time cost (matches `argon2::Params::DEFAULT_T_COST`).
pub const DEFAULT_ARGON2_T_COST: u32 = 2;
/// Default Argon2 parallelism cost (matches `argon2::Params::DEFAULT_P_COST`).
pub const DEFAULT_ARGON2_P_COST: u32 = 1;

/// Arguments for the `create-user` administrative subcommand.
#[derive(Parser, OrthoConfig, Deserialize, Serialize, Default, Debug, Clone)]
#[ortho_config(prefix = "MXD_")]
// Preserve the existing configuration namespace independently of package renames.
#[command(name = "cli-defs")]
pub struct CreateUserArgs {
    /// Username for the new account.
    pub username: Option<String>,
    /// Password for the new account.
    #[serde(skip_serializing)]
    pub password: Option<String>,
}

/// CLI subcommands exposed by `mxd`.
#[derive(Subcommand, Deserialize, Serialize, Debug, Clone)]
pub enum Commands {
    /// Create a new user account.
    #[command(name = "create-user")]
    CreateUser(CreateUserArgs),
}

/// Runtime configuration shared by all binaries.
///
/// The default bind address `0.0.0.0:5500` listens on all interfaces.
/// This is convenient for local development, but production deployments should
/// bind to a specific interface (for example `127.0.0.1`) and sit behind a
/// reverse proxy.
#[derive(Args, OrthoConfig, Serialize, Deserialize, Default, Debug, Clone)]
#[ortho_config(prefix = "MXD_", discovery(config_cli_visible = true))]
pub struct AppConfig {
    /// Server bind address.
    #[ortho_config(default = "0.0.0.0:5500".to_owned())]
    #[arg(long)]
    pub bind: String,
    /// Database connection string or path.
    #[ortho_config(default = "mxd.db".to_owned())]
    #[arg(long)]
    pub database: String,
    /// Optional migration timeout in seconds.
    #[arg(long)]
    pub migration_timeout_secs: Option<u64>,
    /// Argon2 memory cost parameter.
    #[ortho_config(default = DEFAULT_ARGON2_M_COST)]
    #[arg(long)]
    pub argon2_m_cost: u32,
    /// Argon2 time cost parameter.
    #[ortho_config(default = DEFAULT_ARGON2_T_COST)]
    #[arg(long)]
    pub argon2_t_cost: u32,
    /// Argon2 parallelism cost parameter.
    #[ortho_config(default = DEFAULT_ARGON2_P_COST)]
    #[arg(long)]
    pub argon2_p_cost: u32,
}

/// Top-level CLI entry point consumed by binaries.
#[derive(Parser, Serialize)]
#[command(name = "mxd", version)]
pub struct Cli {
    /// CLI configuration overrides (merged with files and defaults at runtime).
    #[command(flatten)]
    config: __AppConfigCli,
    /// Optional subcommand.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

impl Cli {
    /// Resolve defaults, discovered files, environment, and parsed CLI overrides.
    ///
    /// Only the file selector is passed to the generated lower-layer loader.
    /// The already-parsed flatten group supplies the final layer, preserving
    /// explicit values equal to defaults without rescanning subcommand tokens.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use clap::Parser;
    /// use cli_defs::Cli;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let cli = Cli::try_parse_from(["mxd", "--bind", "127.0.0.1:5500"])?;
    /// let config = cli.resolve_config()?;
    /// assert_eq!(config.bind, "127.0.0.1:5500");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns file discovery, environment decoding, or configuration merge errors.
    pub fn resolve_config(&self) -> OrthoResult<AppConfig> {
        let mut selector_args = vec![OsString::from("mxd")];
        if let Some(path) = &self.config.config_path {
            selector_args.push(OsString::from("--config-path"));
            selector_args.push(path.as_os_str().to_owned());
        }
        let (mut layers, errors) = AppConfig::compose_layers_from_iter(selector_args).into_parts();
        layers.push(MergeLayer::cli(Cow::Owned(sanitize_value(&self.config)?)));
        LayerComposition::new(layers, errors).into_merge_result(AppConfig::merge_from_layers)
    }
}
