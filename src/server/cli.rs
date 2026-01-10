//! Command-line interface definitions for the MXD server.
//!
//! Keeping these types in the library allows every binary (legacy TCP and the
//! forthcoming wireframe adapter) to expose an identical configuration surface.

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
// WORKAROUND: Literal defaults for build.rs compatibility
//
// These constants duplicate `argon2::Params::DEFAULT_*` values so that
// `build.rs` can include this module directly for man page generation
// without adding `argon2` as a build-dependency.
//
// This workaround can be removed once ortho-config gains native man page
// generation support, allowing the CLI to be defined with full runtime
// dependencies while still producing man pages at build time.
//
// Values as of argon2 0.5.x:
//   DEFAULT_M_COST = 19_456
//   DEFAULT_T_COST = 2
//   DEFAULT_P_COST = 1
// ────────────────────────────────────────────────────────────────────────────
const DEFAULT_ARGON2_M_COST: u32 = 19_456;
const DEFAULT_ARGON2_T_COST: u32 = 2;
const DEFAULT_ARGON2_P_COST: u32 = 1;

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
#[derive(Args, OrthoConfig, Serialize, Deserialize, Default, Debug, Clone)]
#[ortho_config(prefix = "MXD_")]
pub struct AppConfig {
    /// Server bind address.
    #[ortho_config(default = "0.0.0.0:5500".to_owned())]
    #[arg(long, default_value_t = String::from("0.0.0.0:5500"))]
    pub bind: String,
    /// Database connection string or path.
    #[ortho_config(default = "mxd.db".to_owned())]
    #[arg(long, default_value_t = String::from("mxd.db"))]
    pub database: String,
    /// Argon2 memory cost parameter.
    #[ortho_config(default = DEFAULT_ARGON2_M_COST)]
    #[arg(long, default_value_t = DEFAULT_ARGON2_M_COST)]
    pub argon2_m_cost: u32,
    /// Argon2 time cost parameter.
    #[ortho_config(default = DEFAULT_ARGON2_T_COST)]
    #[arg(long, default_value_t = DEFAULT_ARGON2_T_COST)]
    pub argon2_t_cost: u32,
    /// Argon2 parallelism cost parameter.
    #[ortho_config(default = DEFAULT_ARGON2_P_COST)]
    #[arg(long, default_value_t = DEFAULT_ARGON2_P_COST)]
    pub argon2_p_cost: u32,
}

/// Top-level CLI entry point consumed by binaries.
#[derive(Parser, Deserialize, Serialize, Debug, Clone)]
pub struct Cli {
    /// Application configuration.
    #[command(flatten)]
    pub config: AppConfig,
    /// Optional subcommand.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[cfg(test)]
mod tests {
    use figment::Jail;
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn env_config_loading() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            j.set_env("MXD_DATABASE", "env.db");
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind, "127.0.0.1:8000");
            assert_eq!(cfg.database, "env.db".to_string());
            Ok(())
        });
    }

    #[rstest]
    fn cli_overrides_env() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            let cfg = AppConfig::load_from_iter(["mxd", "--bind", "0.0.0.0:9000"]).expect("load");
            assert_eq!(cfg.bind, "0.0.0.0:9000");
            Ok(())
        });
    }

    #[rstest]
    fn loads_from_dotfile() {
        Jail::expect_with(|j| {
            j.create_file(".mxd.toml", "bind = \"1.2.3.4:1111\"")?;
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind, "1.2.3.4:1111".to_string());
            Ok(())
        });
    }

    #[rstest]
    fn argon2_cli() {
        Jail::expect_with(|_j| {
            let cfg = AppConfig::load_from_iter(["mxd", "--argon2-m-cost", "1024"]).expect("load");
            assert_eq!(cfg.argon2_m_cost, 1024);
            Ok(())
        });
    }
}
