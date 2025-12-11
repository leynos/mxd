//! Integration tests for the `create-user` admin flow against embedded
//! `PostgreSQL`.
//!
//! These scenarios start a disposable cluster via `PostgresTestDb::new()`,
//! exercise `run_command(Commands::CreateUser)` end to end, then verify the
//! user record exists in the database. The helper tears the cluster down
//! automatically on drop. In CI, the suite skips gracefully when the embedded
//! worker binary is unavailable, emitting `SKIP-TEST-CLUSTER` so the failure
//! is visible without breaking the pipeline. The suite also skips when an
//! external `POSTGRES_TEST_URL` is configured because this test targets the
//! embedded workflow specifically.

use anyhow::Result;
use argon2::Params;
use diesel_async::AsyncConnection;
use mxd::{
    db::{DbConnection, get_user_by_name},
    server::{AppConfig, Commands, CreateUserArgs, run_command},
};
use rstest::rstest;
use test_util::postgres::{PostgresTestDb, PostgresTestDbError};
use tokio::runtime::Builder;

#[rstest]
fn create_user_against_embedded_postgres() -> Result<()> {
    if std::env::var_os("POSTGRES_TEST_URL").is_some() {
        eprintln!(concat!(
            "SKIP-TEST-CLUSTER: POSTGRES_TEST_URL set, skipping embedded ",
            "postgres test"
        ));
        return Ok(());
    }

    // PostgresTestDb::new() uses block_on internally when starting an embedded
    // cluster, so must be called outside any tokio runtime to avoid runtime
    // nesting errors.
    let pg = match PostgresTestDb::new() {
        Ok(db) => db,
        Err(PostgresTestDbError::Unavailable(_)) => {
            eprintln!("SKIP-TEST-CLUSTER: PostgreSQL unavailable");
            return Ok(());
        }
        Err(err) => {
            anyhow::bail!("Failed to initialise PostgreSQL test database: {err}");
        }
    };

    let rt = Builder::new_current_thread().enable_all().build()?;

    rt.block_on(async {
        let cfg = AppConfig {
            database: pg.url.to_string(),
            argon2_m_cost: Params::DEFAULT_M_COST,
            argon2_t_cost: Params::DEFAULT_T_COST,
            argon2_p_cost: Params::DEFAULT_P_COST,
            ..AppConfig::default()
        };

        let username = format!("user_{}", rand::random::<u64>());
        let args = CreateUserArgs {
            username: Some(username.clone()),
            password: Some("passw0rd!".to_string()),
        };

        run_command(Commands::CreateUser(args), &cfg).await?;

        let mut conn = DbConnection::establish(&cfg.database).await?;
        let user = get_user_by_name(&mut conn, &username)
            .await?
            .expect("user should be persisted");
        assert_eq!(user.username, username);

        Ok(())
    })
}
