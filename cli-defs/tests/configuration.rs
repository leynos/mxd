//! Real generated-loader contracts with child-local environments and disposable files.

#[path = "configuration/discovery.rs"]
mod discovery;
#[path = "configuration/formats.rs"]
mod formats;
#[path = "configuration/grammar.rs"]
mod grammar;
mod support;

use anyhow::{Context, Result, ensure};
use clap::Parser;
use cli_defs::Cli;
use ortho_config::{
    OrthoError,
    serde_json::{self, Value, json},
};
use rstest::{fixture, rstest};
use support::{ConfigFixture, ENVIRONMENT, defaults, environment_values, file_values};

#[fixture]
fn fixture() -> Result<ConfigFixture> {
    let fixture = ConfigFixture::new()?;
    Ok(fixture)
}

/// Executable composition boundary for the isolated test process.
#[test]
fn configuration_probe() -> Result<()> {
    let Ok(expected_text) = std::env::var("MXD_TEST_EXPECTED") else {
        return Ok(());
    };
    let args: Vec<String> = serde_json::from_str(&std::env::var("MXD_TEST_ARGS")?)?;
    let expected: Value = serde_json::from_str(&expected_text)?;
    let cli = Cli::try_parse_from(args)?;
    let resolved = cli.resolve_config();
    if expected == json!("file-error") {
        let error = resolved.err().context("broken selected file must fail")?;
        ensure!(contains_file_error(&error), "expected typed file error");
    } else if expected == json!("provider-error") {
        let error = resolved.err().context("invalid provider data must fail")?;
        ensure!(
            contains_provider_error(&error),
            "configuration contract failed"
        );
    } else {
        ensure!(
            serde_json::to_value(resolved.context("resolve configuration")?)? == expected,
            "configuration values differ"
        );
    }
    Ok(())
}

fn contains_file_error(error: &OrthoError) -> bool {
    match error {
        OrthoError::File { .. } => true,
        OrthoError::Aggregate(errors) => errors.iter().any(contains_file_error),
        _ => false,
    }
}

fn contains_provider_error(error: &OrthoError) -> bool {
    match error {
        OrthoError::Gathering(_) | OrthoError::Merge { .. } => true,
        OrthoError::Aggregate(errors) => errors.iter().any(contains_provider_error),
        _ => false,
    }
}

#[rstest]
fn missing_optional_file_uses_defaults(fixture: Result<ConfigFixture>) -> Result<()> {
    fixture?.probe(&["mxd"], &[], &defaults())
}

#[rstest]
fn file_overrides_defaults_and_omitted_cli_fields(fixture: Result<ConfigFixture>) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write(".mxd.toml", include_str!("fixtures/config.toml"))?;
    isolated.probe(&["mxd"], &[], &file_values())
}

#[rstest]
fn environment_overrides_defaults(fixture: Result<ConfigFixture>) -> Result<()> {
    fixture?.probe(&["mxd"], ENVIRONMENT, &environment_values())
}

#[rstest]
fn environment_overrides_file_for_every_field(fixture: Result<ConfigFixture>) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write(".mxd.toml", include_str!("fixtures/config.toml"))?;
    isolated.probe(&["mxd"], ENVIRONMENT, &environment_values())
}

#[rstest]
fn explicit_cli_overrides_file_and_environment(fixture: Result<ConfigFixture>) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write(".mxd.toml", include_str!("fixtures/config.toml"))?;
    isolated.probe(
        &[
            "mxd",
            "--bind",
            "127.0.0.1:7300",
            "--database",
            "cli.db",
            "--migration-timeout-secs",
            "9",
            "--argon2-m-cost",
            "4096",
            "--argon2-t-cost",
            "5",
            "--argon2-p-cost",
            "4",
        ],
        ENVIRONMENT,
        &json!({
            "bind": "127.0.0.1:7300", "database": "cli.db", "migration_timeout_secs": 9,
            "argon2_m_cost": 4096, "argon2_t_cost": 5, "argon2_p_cost": 4,
        }),
    )
}

#[rstest]
fn explicit_default_values_still_override_lower_layers(
    fixture: Result<ConfigFixture>,
) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write(".mxd.toml", include_str!("fixtures/config.toml"))?;
    let expected = json!({
        "bind": "0.0.0.0:5500", "database": "mxd.db", "migration_timeout_secs": 0,
        "argon2_m_cost": 19456, "argon2_t_cost": 2, "argon2_p_cost": 1,
    });
    isolated.probe(
        &[
            "mxd",
            "--bind",
            "0.0.0.0:5500",
            "--database",
            "mxd.db",
            "--migration-timeout-secs",
            "0",
            "--argon2-m-cost",
            "19456",
            "--argon2-t-cost",
            "2",
            "--argon2-p-cost",
            "1",
        ],
        ENVIRONMENT,
        &expected,
    )
}

#[rstest]
#[case::database("--database", "create-user", "database")]
#[case::bind("--bind", "create-user", "bind")]
fn command_name_values_survive_selected_subcommand(
    fixture: Result<ConfigFixture>,
    #[case] flag: &str,
    #[case] value: &str,
    #[case] key: &str,
) -> Result<()> {
    let isolated = fixture?;
    let mut expected = defaults();
    *expected.get_mut(key).context("known configuration key")? = json!(value);
    isolated.probe(
        &[
            "mxd",
            flag,
            value,
            "create-user",
            "disposable-user",
            "disposable-password",
        ],
        &[],
        &expected,
    )
}

#[rstest]
fn unspecified_cli_fields_preserve_timeout_and_argon2(
    fixture: Result<ConfigFixture>,
) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write(".mxd.toml", include_str!("fixtures/config.toml"))?;
    let mut expected = file_values();
    *expected.get_mut("database").context("database key")? = json!("cli.db");
    isolated.probe(
        &["mxd", "--database", "cli.db", "create-user"],
        &[],
        &expected,
    )
}
