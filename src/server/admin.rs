//! Administrative command handlers shared across server runtimes.
//!
//! These helpers keep user-management workflows available regardless of the
//! selected networking adapter. The functions remain free of transport
//! dependencies so both the legacy Tokio loop and the Wireframe runtime can
//! reuse them.

#![allow(
    clippy::shadow_reuse,
    reason = "intentional shadowing for config merging"
)]
#![allow(
    clippy::print_stdout,
    reason = "intentional user output for CLI commands"
)]

use anyhow::{Context, Result, anyhow};
use argon2::{Algorithm, Argon2, ParamsBuilder, Version};
use diesel_async::AsyncConnection;
use ortho_config::load_and_merge_subcommand_for;

use super::{AppConfig, Commands, CreateUserArgs};
use crate::{
    db::{DbConnection, apply_migrations, create_user},
    models,
    users::hash_password,
};

/// Execute an administrative command.
///
/// # Errors
///
/// Propagates failures from configuration merging or database operations.
pub async fn run_command(command: Commands, cfg: &AppConfig) -> Result<()> {
    match command {
        Commands::CreateUser(args) => {
            let args = load_and_merge_subcommand_for::<CreateUserArgs>(&args)?;
            run_create_user(args, cfg).await
        }
    }
}

/// Build an Argon2 instance using the supplied configuration parameters.
///
/// # Errors
///
/// Returns any error emitted while constructing the Argon2 parameter set.
pub fn argon2_from_config(cfg: &AppConfig) -> Result<Argon2<'static>> {
    let params = ParamsBuilder::new()
        .m_cost(cfg.argon2_m_cost)
        .t_cost(cfg.argon2_t_cost)
        .p_cost(cfg.argon2_p_cost)
        .build()
        .with_context(|| {
            format!(
                "invalid Argon2 params derived from config: m_cost={}, t_cost={}, p_cost={}",
                cfg.argon2_m_cost, cfg.argon2_t_cost, cfg.argon2_p_cost
            )
        })?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

async fn run_create_user(args: CreateUserArgs, cfg: &AppConfig) -> Result<()> {
    let username = args.username.ok_or_else(|| anyhow!("missing username"))?;
    let password = args.password.ok_or_else(|| anyhow!("missing password"))?;

    let argon2 = argon2_from_config(cfg)?;
    let hashed = hash_password(&argon2, &password)?;
    let new_user = models::NewUser {
        username: &username,
        password: &hashed,
    };
    let mut conn = DbConnection::establish(&cfg.database).await?;
    apply_migrations(&mut conn, &cfg.database).await?;
    create_user(&mut conn, &new_user)
        .await
        .with_context(|| format!("failed to create user '{username}'"))?;
    println!("User {username} created");
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn argon2_respects_cli_overrides() {
        let cfg = AppConfig {
            argon2_m_cost: 1024,
            argon2_t_cost: 5,
            argon2_p_cost: 3,
            ..AppConfig::default()
        };

        let argon2 = argon2_from_config(&cfg).expect("argon2");

        let params = argon2.params();
        assert_eq!(params.m_cost(), cfg.argon2_m_cost);
        assert_eq!(params.t_cost(), cfg.argon2_t_cost);
        assert_eq!(params.p_cost(), cfg.argon2_p_cost);
    }

    #[rstest]
    #[case(None, Some("password".into()), "missing username")]
    #[case(Some("user".into()), None, "missing password")]
    #[tokio::test]
    async fn run_command_rejects_missing_fields(
        #[case] username: Option<String>,
        #[case] password: Option<String>,
        #[case] expected: &str,
    ) {
        let cfg = AppConfig::default();
        let args = CreateUserArgs { username, password };

        let err = run_command(Commands::CreateUser(args), &cfg)
            .await
            .expect_err("command must fail");

        assert!(err.to_string().contains(expected));
    }
}
