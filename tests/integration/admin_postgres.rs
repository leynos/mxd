//! Integration tests for admin commands backed by embedded `PostgreSQL`.

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
            let message = err.to_string();
            if message.contains("SKIP-TEST-CLUSTER") || message.contains("PG_EMBEDDED_WORKER") {
                eprintln!("{message}");
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
