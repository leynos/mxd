//! Parsed flatten-group and positional subcommand grammar contracts.

use clap::{CommandFactory, Parser, error::ErrorKind};
use cli_defs::{Cli, Commands, CreateUserArgs};
use ortho_config::serde_json;
use proptest::prelude::*;
use rstest::rstest;

#[rstest]
fn generated_flatten_group_has_no_conflicting_flags() { Cli::command().debug_assert(); }

#[rstest]
fn selected_subcommand_keeps_existing_configuration_namespace() {
    assert_eq!(CreateUserArgs::command().get_name(), "cli-defs");
}

#[rstest]
fn separator_preserves_hyphen_prefixed_credentials_without_serializing_password() {
    let parsed = Cli::try_parse_from([
        "mxd",
        "create-user",
        "--",
        "-disposable-user",
        "-disposable-password",
    ])
    .expect("positional credential separator");
    let Some(Commands::CreateUser(args)) = parsed.command else {
        panic!("selected command missing")
    };
    assert_eq!(args.username.as_deref(), Some("-disposable-user"));
    assert_eq!(args.password.as_deref(), Some("-disposable-password"));
    let serialized = serde_json::to_value(args).expect("serialize public credential fields");
    assert!(serialized.get("password").is_none());
}

#[rstest]
fn global_options_after_subcommand_remain_rejected() {
    let error = Cli::try_parse_from([
        "mxd",
        "create-user",
        "user",
        "disposable-password",
        "--database",
        "late.db",
    ])
    .err()
    .expect("global arguments belong before subcommand");
    assert_eq!(error.kind(), ErrorKind::UnknownArgument);
}

#[rstest]
fn outer_separator_does_not_create_a_subcommand() {
    let error = Cli::try_parse_from(["mxd", "--", "create-user"])
        .err()
        .expect("outer parser has no positional arguments");
    assert_eq!(error.kind(), ErrorKind::UnknownArgument);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn database_value_tokens_are_preserved(value in "[a-zA-Z0-9 ._-]{1,48}") {
        let parsed = Cli::try_parse_from(["mxd", &format!("--database={value}"), "create-user", "user", "disposable-password"]);
        prop_assert!(parsed.is_ok());
        let cli = parsed.map_err(|error| TestCaseError::fail(error.to_string()))?;
        let serialized = serde_json::to_value(cli).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(serialized.get("config").and_then(|config| config.get("database")).and_then(serde_json::Value::as_str), Some(value.as_str()));
    }
}
