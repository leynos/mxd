//! BDD-style integration tests for the create-user command.
//!
//! These tests exercise the CLI-driven create-user workflow against a temporary
//! SQLite database, verifying both successful user creation and error handling.

#![cfg(feature = "sqlite")]

use std::cell::RefCell;

use anyhow::Result;
use argon2::Params;
use diesel_async::AsyncConnection;
use mxd::{
    db::{self, DbConnection},
    server::{self, AppConfig, Cli, Commands, CreateUserArgs},
};
use rstest::fixture;
use rstest_bdd::{assert_step_err, assert_step_ok};
use rstest_bdd_macros::{given, scenario, then, when};
use tempfile::TempDir;
use tokio::runtime::Runtime;

type CommandResult = Result<()>;

struct CreateUserWorld {
    _temp_dir: TempDir,
    config: RefCell<AppConfig>,
    outcome: RefCell<Option<CommandResult>>,
    rt: Runtime,
}

impl CreateUserWorld {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("tempdir");
        let db_path = temp_dir.path().join("bdd.mxd.db");
        let config = AppConfig {
            database: db_path.to_string_lossy().into_owned(),
            bind: "127.0.0.1:0".to_string(),
            argon2_m_cost: Params::DEFAULT_M_COST,
            argon2_t_cost: Params::DEFAULT_T_COST,
            argon2_p_cost: Params::DEFAULT_P_COST,
            ..AppConfig::default()
        };
        let rt = Runtime::new().expect("runtime");
        Self {
            _temp_dir: temp_dir,
            config: RefCell::new(config),
            outcome: RefCell::new(None),
            rt,
        }
    }

    fn database_path(&self) -> String { self.config.borrow().database.clone() }

    fn run_command(&self, username: String, password: Option<String>) {
        let args = CreateUserArgs {
            username: Some(username),
            password,
        };
        let cli = Cli {
            config: self.config.borrow().clone(),
            command: Some(Commands::CreateUser(args)),
        };
        let result = self.rt.block_on(server::run_with_cli(cli));
        self.outcome.borrow_mut().replace(result);
    }

    fn assert_user_exists(&self, username: &str) {
        let db = self.database_path();
        let lookup = username.to_string();
        let fetched = self.rt.block_on(async move {
            let mut conn = DbConnection::establish(&db).await.expect("db conn");
            db::get_user_by_name(&mut conn, &lookup)
                .await
                .expect("query")
        });
        let found = fetched.map(|u| u.username);
        assert_eq!(found.as_deref(), Some(username));
    }

    fn assert_failure_contains(&self, message: &str) {
        let outcome_ref = self.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("command not executed");
        };
        let status = outcome
            .as_ref()
            .map(|()| ())
            .map_err(std::string::ToString::to_string);
        let text = assert_step_err!(status);
        assert!(
            text.contains(message),
            "expected error to contain '{message}', got '{text}'"
        );
    }
}

#[fixture]
fn world() -> CreateUserWorld {
    let world = CreateUserWorld::new();
    world
}

#[given("a temporary sqlite database")]
fn given_temp_db(world: &CreateUserWorld) {
    let binding = world.database_path();
    let path = std::path::Path::new(&binding);
    assert!(path.parent().is_some());
}

#[given("server configuration bound to that database")]
fn given_config_bound(world: &CreateUserWorld) { let _ = world.database_path(); }

#[when("the operator runs create-user with username \"{username}\" and password \"{password}\"")]
fn when_run_with_password(world: &CreateUserWorld, username: String, password: String) {
    world.run_command(username, Some(password));
}

#[when("the operator runs create-user with username \"{username}\" and no password")]
fn when_run_without_password(world: &CreateUserWorld, username: String) {
    world.run_command(username, None);
}

#[then("the command completes successfully")]
fn then_success(world: &CreateUserWorld) {
    let outcome_ref = world.outcome.borrow();
    let Some(outcome) = outcome_ref.as_ref() else {
        panic!("command not executed");
    };
    let status = outcome
        .as_ref()
        .map(|()| ())
        .map_err(std::string::ToString::to_string);
    assert_step_ok!(status);
}

#[then("the database contains a user named \"{username}\"")]
fn then_user_exists(world: &CreateUserWorld, username: String) {
    world.assert_user_exists(&username);
}

#[then("the command fails with message \"{message}\"")]
fn then_failure(world: &CreateUserWorld, message: String) {
    world.assert_failure_contains(&message);
}

#[scenario(path = "tests/features/create_user_command.feature", index = 0)]
fn create_user_happy(world: CreateUserWorld) { drop(world); }

#[scenario(path = "tests/features/create_user_command.feature", index = 1)]
fn create_user_missing_password(world: CreateUserWorld) { drop(world); }
