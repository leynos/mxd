//! Command line interface for the server.
//!
//! Provides subcommands to start the daemon, manage the database and
//! create users. This is the primary entry point used when running `mxd`.
#![allow(non_snake_case)]

use anyhow::Result;
use argon2::{Algorithm, Argon2, Params, ParamsBuilder, Version};
use clap::{Args, Parser, Subcommand};
use clap_dispatch::clap_dispatch;
use diesel_async::AsyncConnection;
use mxd::{
    db::{DbConnection, apply_migrations, create_user},
    models,
    transport::legacy::{LegacyServerConfig, run as run_legacy_server},
    users::hash_password,
};
use ortho_config::{OrthoConfig, load_and_merge_subcommand_for};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

#[derive(Parser, Deserialize, Serialize, Default, Debug, Clone, OrthoConfig)]
#[ortho_config(prefix = "MXD_")]
struct CreateUserArgs {
    username: Option<String>,
    password: Option<String>,
}

#[clap_dispatch(fn run(self, cfg: &AppConfig) -> Result<()>)]
#[derive(Subcommand, Deserialize, Serialize, Debug, Clone)]
enum Commands {
    #[command(name = "create-user")]
    CreateUser(CreateUserArgs),
}

impl Run for CreateUserArgs {
    /// Creates a new user with the specified username and password, hashing the password securely
    /// and storing the user in the database.
    ///
    /// Validates that both username and password are provided, hashes the password using Argon2id
    /// with parameters from the configuration, runs database migrations if necessary, and inserts
    /// the new user record. Prints a confirmation message upon successful creation.
    ///
    /// # Errors
    ///
    /// Returns an error if required arguments are missing, password hashing fails, database
    /// connection or migrations fail, or user creation is unsuccessful.
    fn run(self, cfg: &AppConfig) -> Result<()> {
        Handle::current().block_on(async {
            let username = self
                .username
                .ok_or_else(|| anyhow::anyhow!("missing username"))?;
            let password = self
                .password
                .ok_or_else(|| anyhow::anyhow!("missing password"))?;

            let params = ParamsBuilder::new()
                .m_cost(cfg.argon2_m_cost)
                .t_cost(cfg.argon2_t_cost)
                .p_cost(cfg.argon2_p_cost)
                .build()?;
            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
            let hashed = hash_password(&argon2, &password)?;
            let new_user = models::NewUser {
                username: &username,
                password: &hashed,
            };
            let mut conn = DbConnection::establish(&cfg.database).await?;
            apply_migrations(&mut conn, &cfg.database).await?;
            create_user(&mut conn, &new_user).await?;
            println!("User {username} created");
            Ok(())
        })
    }
}

#[allow(non_snake_case)]
#[derive(Args, OrthoConfig, Serialize, Deserialize, Default, Debug, Clone)]
#[ortho_config(prefix = "MXD_")]
struct AppConfig {
    #[ortho_config(default = "0.0.0.0:5500".to_string())]
    #[arg(long, default_value_t = String::from("0.0.0.0:5500"))]
    bind: String,
    #[ortho_config(default = "mxd.db".to_string())]
    #[arg(long, default_value_t = String::from("mxd.db"))]
    database: String,
    #[ortho_config(default = Params::DEFAULT_M_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_M_COST)]
    argon2_m_cost: u32,
    #[ortho_config(default = Params::DEFAULT_T_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_T_COST)]
    argon2_t_cost: u32,
    #[ortho_config(default = Params::DEFAULT_P_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_P_COST)]
    argon2_p_cost: u32,
}

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    config: AppConfig,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = cli.config;
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::CreateUser(args) => {
                let args = load_and_merge_subcommand_for::<CreateUserArgs>(&args)?;
                return args.run(&cfg);
            }
        }
    }

    let params = ParamsBuilder::new()
        .m_cost(cfg.argon2_m_cost)
        .t_cost(cfg.argon2_t_cost)
        .p_cost(cfg.argon2_p_cost)
        .build()?;
    // Placeholder: use customized Argon2 instance when creating accounts
    let _argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let server_cfg = LegacyServerConfig::from_raw(&cfg.bind, &cfg.database)
        .map_err(|err| anyhow::anyhow!(err))?;
    run_legacy_server(&server_cfg).await
}

#[cfg(test)]
mod tests {
    use figment::Jail;

    use super::*;

    #[test]
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

    #[test]
    fn cli_overrides_env() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            let cfg = AppConfig::load_from_iter(["mxd", "--bind", "0.0.0.0:9000"]).expect("load");
            assert_eq!(cfg.bind, "0.0.0.0:9000");
            Ok(())
        });
    }

    #[test]
    fn loads_from_dotfile() {
        Jail::expect_with(|j| {
            j.create_file(".mxd.toml", "bind = \"1.2.3.4:1111\"")?;
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind, "1.2.3.4:1111".to_string());
            Ok(())
        });
    }

    #[test]
    fn argon2_cli() {
        Jail::expect_with(|_j| {
            let cfg = AppConfig::load_from_iter(["mxd", "--argon2-m-cost", "1024"]).expect("load");
            assert_eq!(cfg.argon2_m_cost, 1024);
            Ok(())
        });
    }
}
