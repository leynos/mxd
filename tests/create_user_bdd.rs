//! BDD-style integration tests for the create-user command.
//!
//! These tests exercise the CLI-driven create-user workflow against a temporary
//! `SQLite` database, verifying both successful user creation and error handling.

#![cfg(feature = "sqlite")]

use std::cell::RefCell;

use anyhow::{Context, Result, anyhow};
use argon2::Params;
use diesel_async::AsyncConnection;
use mxd::{
    db::{self, DbConnection},
    server::{self, AppConfig, Commands, CreateUserArgs, ResolvedCli},
};
use rstest::fixture;
use rstest_bdd::{assert_step_err, assert_step_ok};
use rstest_bdd_macros::{given, scenarios, then, when};
use tempfile::TempDir;
use tokio::runtime::Runtime;

#[derive(Debug, Clone)]
struct Username(String);

impl From<String> for Username {
    fn from(s: String) -> Self { Self(s) }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str { &self.0 }
}

impl std::str::FromStr for Username {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(Self(s.to_owned())) }
}

#[derive(Debug, Clone)]
struct Password(String);

impl From<String> for Password {
    fn from(s: String) -> Self { Self(s) }
}

impl Password {
    fn into_inner(self) -> String { self.0 }
}

impl std::str::FromStr for Password {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(Self(s.to_owned())) }
}

type CommandResult = Result<()>;

struct CreateUserWorld {
    _temp_dir: TempDir,
    config: RefCell<AppConfig>,
    outcome: RefCell<Option<CommandResult>>,
    rt: Runtime,
}

impl CreateUserWorld {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new().context("failed to create tempdir for test")?;
        let db_path = temp_dir.path().join("bdd.mxd.db");
        let config = AppConfig {
            database: db_path.to_string_lossy().into_owned(),
            bind: "127.0.0.1:0".to_owned(),
            argon2_m_cost: Params::DEFAULT_M_COST,
            argon2_t_cost: Params::DEFAULT_T_COST,
            argon2_p_cost: Params::DEFAULT_P_COST,
        };
        let rt = Runtime::new().context("failed to create runtime for test")?;
        Ok(Self {
            _temp_dir: temp_dir,
            config: RefCell::new(config),
            outcome: RefCell::new(None),
            rt,
        })
    }

    fn database_path(&self) -> String { self.config.borrow().database.clone() }

    fn run_command(&self, username: Username, password: Option<Password>) {
        let password_value = password.map(|value| {
            let inner = value.into_inner();
            Password(inner)
        });
        let args = CreateUserArgs {
            username: Some(username.0),
            password: password_value.map(|p| p.0),
        };
        let cli = ResolvedCli {
            config: self.config.borrow().clone(),
            command: Some(Commands::CreateUser(args)),
        };
        let result = self.rt.block_on(server::run_with_cli(cli));
        self.outcome.borrow_mut().replace(result);
    }

    fn assert_user_exists(&self, username: &Username) -> Result<()> {
        let db = self.database_path();
        let lookup = username.as_ref().to_owned();
        let fetched = self.rt.block_on(async move {
            let mut conn = DbConnection::establish(&db)
                .await
                .context("failed to establish db connection")?;
            db::get_user_by_name(&mut conn, &lookup)
                .await
                .context("failed to query user")
        })?;
        let found = fetched.map(|u| u.username);
        if found.as_deref() != Some(username.as_ref()) {
            return Err(anyhow!(
                "expected user '{}' to exist, found {found:?}",
                username.as_ref()
            ));
        }
        Ok(())
    }

    fn assert_failure_contains(&self, message: &str) {
        let outcome_ref = self.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("command not executed");
        };
        let status = outcome.as_ref().map_err(ToString::to_string);
        let text = assert_step_err!(status);
        assert!(
            text.contains(message),
            "expected error to contain '{message}', got '{text}'"
        );
    }
}

#[fixture]
fn world() -> CreateUserWorld {
    let world = CreateUserWorld::new().unwrap_or_else(|err| {
        panic!("failed to create test world: {err}");
    });
    // Sanity-check fixture invariants so step definitions can rely on the DB path shape.
    assert!(
        world.config.borrow().database.ends_with("bdd.mxd.db"),
        "fixture must create a temporary sqlite database"
    );
    world
}

#[given("a temporary sqlite database")]
fn given_temp_db(world: &CreateUserWorld) {
    let binding = world.database_path();
    let path = std::path::Path::new(&binding);
    assert!(path.parent().is_some());
}

#[given("server configuration bound to that database")]
fn given_config_bound(world: &CreateUserWorld) {
    let db_path = world.database_path();
    assert!(
        db_path.ends_with("bdd.mxd.db"),
        "temporary sqlite database path must end with bdd.mxd.db"
    );
}

#[when("the operator runs create-user with username \"{username}\" and password \"{password}\"")]
fn when_run_with_password(world: &CreateUserWorld, username: Username, password: Password) {
    world.run_command(username, Some(password));
}

#[when("the operator runs create-user with username \"{username}\" and no password")]
fn when_run_without_password(world: &CreateUserWorld, username: Username) {
    world.run_command(username, None);
}

#[then("the command completes successfully")]
fn then_success(world: &CreateUserWorld) {
    let outcome_ref = world.outcome.borrow();
    let Some(outcome) = outcome_ref.as_ref() else {
        panic!("command not executed");
    };
    let status = outcome.as_ref().map_err(ToString::to_string);
    assert_step_ok!(status);
}

#[then("the database contains a user named \"{username}\"")]
fn then_user_exists(world: &CreateUserWorld, username: Username) {
    if let Err(err) = world.assert_user_exists(&username) {
        panic!("user existence check failed: {err}");
    }
}

#[then("the command fails with message \"{message}\"")]
fn then_failure(world: &CreateUserWorld, message: String) {
    world.assert_failure_contains(&message);
}

scenarios!(
    "tests/features",
    tags = "@create-user",
    fixtures = [world: CreateUserWorld]
);
