//! Command-line interface definitions for the MXD server.
//!
//! Keeping these types in the library allows every binary (legacy TCP and the
//! forthcoming wireframe adapter) to expose an identical configuration surface.

#![expect(
    non_snake_case,
    reason = "Clap/OrthoConfig derive macros generate helper modules with uppercase names"
)]
#![allow(
    missing_docs,
    reason = "OrthoConfig and Clap derive macros generate items that cannot be documented"
)]
#![allow(
    unfulfilled_lint_expectations,
    reason = "derive macros conditionally generate items"
)]

use argon2::Params;
use clap::{Args, Parser, Subcommand};
use ortho_config::OrthoConfig;
use serde::{Deserialize, Serialize};

/// Arguments for the `create-user` administrative subcommand.
#[expect(
    missing_docs,
    reason = "OrthoConfig derive macro generates items that cannot be documented"
)]
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
#[expect(
    missing_docs,
    reason = "OrthoConfig derive macro generates items that cannot be documented"
)]
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
    #[ortho_config(default = Params::DEFAULT_M_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_M_COST)]
    pub argon2_m_cost: u32,
    /// Argon2 time cost parameter.
    #[ortho_config(default = Params::DEFAULT_T_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_T_COST)]
    pub argon2_t_cost: u32,
    /// Argon2 parallelism cost parameter.
    #[ortho_config(default = Params::DEFAULT_P_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_P_COST)]
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
