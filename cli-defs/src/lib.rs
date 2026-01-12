//! Shared CLI type definitions for mxd build and runtime.
//!
//! This crate provides CLI argument and configuration types used by both the
//! `build.rs` script (for man page generation) and the runtime binaries.
//! Extracting these types into a separate crate avoids brittle `#[path = ...]`
//! includes and keeps build-time and runtime dependencies cleanly separated.

// FIXME: File-wide suppressions are unavoidable here. Clap and OrthoConfig derive macros
// inject generated code throughout the module, and there is no mechanism to narrow
// the scope without restructuring the crate.
#![expect(
    non_snake_case,
    reason = "Clap/OrthoConfig derive macros generate helper modules with uppercase names"
)]
#![expect(
    missing_docs,
    reason = "OrthoConfig and Clap derive macros generate items that cannot be documented"
)]

use clap::{Args, Parser, Subcommand};
use ortho_config::OrthoConfig;
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
pub struct CreateUserArgs {
    /// Username for the new account.
    pub username: Option<String>,
    /// Password for the new account.
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
#[ortho_config(prefix = "MXD_")]
pub struct AppConfig {
    /// Server bind address.
    #[ortho_config(default = "0.0.0.0:5500".to_owned())]
    #[arg(long)]
    pub bind: String,
    /// Database connection string or path.
    #[ortho_config(default = "mxd.db".to_owned())]
    #[arg(long)]
    pub database: String,
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
pub struct Cli {
    /// CLI configuration overrides (merged with files and defaults at runtime).
    #[command(flatten)]
    pub config: AppConfigCli,
    /// Optional subcommand.
    #[command(subcommand)]
    pub command: Option<Commands>,
}
