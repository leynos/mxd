//! Integration tests for the `create-user` admin flow against embedded
//! `PostgreSQL`.
//!
//! These scenarios start a disposable cluster via `pg_embedded_setup_unpriv`,
//! exercise `run_command(Commands::CreateUser)` end to end, then verify the
//! user record exists in the database. The helper tears the cluster down
//! automatically on drop. In CI, the suite skips gracefully when the embedded
//! worker binary is unavailable (for example, if `PG_EMBEDDED_WORKER` is not
//! set), emitting `SKIP-TEST-CLUSTER` so the failure is visible without
//! breaking the pipeline.

use std::error::Error;

use anyhow::Result;
use diesel_async::AsyncConnection;
use mxd::{
    db::{DbConnection, get_user_by_name},
    server::{AppConfig, Commands, CreateUserArgs, run_command},
};
use pg_embedded_setup_unpriv::TestCluster;
use rstest::rstest;

#[rstest]
#[tokio::test]
async fn create_user_against_embedded_postgres() -> Result<()> {
    let cluster = match TestCluster::new() {
        Ok(cluster) => cluster,
        Err(err) => {
            if let Some(reason) = cluster_skip_reason(&err) {
                eprintln!("{reason}");
                return Ok(());
            }
            return Err(err.into());
        }
    };

    let connection = cluster.connection();
    let database_url = connection.database_url("mxd_admin_test");

    let cfg = AppConfig {
        database: database_url,
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
}

fn cluster_skip_reason(err: &dyn Error) -> Option<String> {
    let mut cause: Option<&dyn Error> = Some(err);
    while let Some(current) = cause {
        let msg = current.to_string();
        if msg.starts_with("SKIP-TEST-CLUSTER") {
            return Some(msg);
        }
        if msg.contains("PG_EMBEDDED_WORKER") {
            return Some(format!("SKIP-TEST-CLUSTER: {msg}"));
        }
        cause = current.source();
    }
    None
}
