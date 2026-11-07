//! Administrative command handlers shared across server runtimes.
//!
//! These helpers keep user-management workflows available regardless of the
//! selected networking adapter. The functions remain free of transport
//! dependencies so both the legacy Tokio loop and the Wireframe runtime can
//! reuse them.

#![expect(
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
            let cli_args = args;
            let mut merged = load_and_merge_subcommand_for::<CreateUserArgs>(&cli_args)?;
            if let Some(password) = cli_args.password.clone() {
                // Preserve CLI overrides because password is excluded from serialization.
                merged.password = Some(password);
            }
            run_create_user(merged, cfg).await
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

/// Create a user from the parsed command line.
///
/// On the `sqlite` feature the linked library is audited before migrations run
/// and before the row is inserted, so an unsupported `SQLite` fails the command
/// without applying migrations or storing the user. The connection is opened
/// first, which can create a missing database file.
///
/// # Errors
///
/// Returns an error if the username or password is missing, if the password
/// cannot be hashed, if the `SQLite` capability audit fails, or if the
/// migration or insert fails.
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
    // Audit before migrating or writing, so an incompatible library fails
    // before it can touch the database.
    #[cfg(feature = "sqlite")]
    crate::db::audit_sqlite_features(&mut conn)
        .await
        .context("SQLite capability audit failed")?;
    apply_migrations(&mut conn, &cfg.database, cfg.migration_timeout_secs).await?;
    create_user(&mut conn, &new_user)
        .await
        .with_context(|| format!("failed to create user '{username}'"))?;
    println!("User {username} created");
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Tests for this module.
    #[cfg(feature = "sqlite")]
    use diesel::{QueryableByName, sql_query, sql_types::BigInt};
    #[cfg(feature = "sqlite")]
    use diesel_async::RunQueryDsl;
    use rstest::rstest;
    #[cfg(feature = "sqlite")]
    use tempfile::TempDir;

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

        // Validate missing-field checks directly to avoid process-wide
        // environment/config merge races during `cargo test` parallel runs.
        let err = run_create_user(args, &cfg)
            .await
            .expect_err("command must fail");

        assert!(err.to_string().contains(expected));
    }

    /// Count the rows in a table, or return `None` when it does not exist.
    ///
    /// The count is taken through `sqlite_master` first so that a missing table
    /// reads as "nothing was written" rather than as a query error. `COUNT(*)`
    /// always yields a row, so `sql_type = BigInt` is correct here even though
    /// Diesel's `Row::get` may otherwise misread an aggregate.
    #[cfg(feature = "sqlite")]
    async fn count_rows(conn: &mut DbConnection, table: &str) -> Option<i64> {
        #[derive(QueryableByName)]
        struct RowCount {
            #[diesel(sql_type = BigInt)]
            total: i64,
        }

        #[derive(QueryableByName)]
        struct TableExists {
            #[diesel(sql_type = BigInt)]
            present: i64,
        }

        let exists = sql_query(
            "SELECT COUNT(*) AS present FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind::<diesel::sql_types::Text, _>(table)
        .get_result::<TableExists>(conn)
        .await
        .expect("sqlite_master must be queryable");
        if exists.present == 0 {
            return None;
        }

        let query = format!("SELECT COUNT(*) AS total FROM \"{table}\"");
        let count = sql_query(query)
            .get_result::<RowCount>(conn)
            .await
            .expect("row count must be readable once the table exists");
        Some(count.total)
    }

    /// An audit failure must abort `create-user` before it writes anything.
    ///
    /// `run_command_rejects_missing_fields` returns before the connection is
    /// opened, so no existing test reaches the audit. Deleting the audit call,
    /// or moving it after `apply_migrations` or `create_user`, would leave the
    /// suite green. This test injects a probe failure and checks the two
    /// observable consequences: the command reports the audit context, and the
    /// database is left without a schema and without a user.
    ///
    /// The injected failure is thread-local and is read before the audit's
    /// first `await`, so it cannot leak into a concurrently running test.
    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn create_user_aborts_before_writing_when_the_audit_fails() {
        let temp_dir = TempDir::new().expect("temporary directory must be created");
        let database = temp_dir.path().join("audited.mxd.db");
        let cfg = AppConfig {
            database: database.to_string_lossy().into_owned(),
            // Cheap Argon2 parameters: hashing happens before the audit, and
            // this test is about ordering, not key stretching strength.
            argon2_m_cost: 1024,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
            ..AppConfig::default()
        };
        let args = CreateUserArgs {
            username: Some("audited".to_owned()),
            password: Some("secret".to_owned()),
        };

        let error = {
            let _injection = crate::db::fail_next_audit();
            run_create_user(args, &cfg)
                .await
                .expect_err("an audit failure must abort the command")
        };

        assert!(
            format!("{error:#}").contains("SQLite capability audit failed"),
            "the command must report the audit context: {error:#}"
        );

        let mut conn = DbConnection::establish(&cfg.database)
            .await
            .expect("the database file must be openable");
        assert_eq!(
            count_rows(&mut conn, "__diesel_schema_migrations").await,
            None,
            "no migration may run when the audit fails"
        );
        assert_eq!(
            count_rows(&mut conn, "users").await,
            None,
            "no user table may exist when the audit fails"
        );
    }
}
