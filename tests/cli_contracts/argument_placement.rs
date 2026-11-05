//! Process-level argument placement and subcommand boundary regressions.

use super::*;

/// Global options before the subcommand reach it, and a command name used as an
/// option value stays a value.
#[rstest]
#[case::command_name_as_database_value(
    &["--database", "create-user", "create-user", "alice"],
    "missing password"
)]
#[case::command_name_value_without_subcommand(
    &["--database", "create-user", "--argon2-p-cost", "0"],
    "invalid Argon2 params derived from config"
)]
#[case::options_before_subcommand(
    &["--database", "create-user", "--bind", "127.0.0.1:5500", "create-user", "alice"],
    "missing password"
)]
#[case::equals_form_and_separator_after_subcommand(
    &["--database=create-user", "create-user", "--", "create-user"],
    "missing password"
)]
#[case::separated_value_and_separator_after_subcommand(
    &["--database", "create-user", "create-user", "--", "create-user"],
    "missing password"
)]
fn global_options_and_subcommand_arguments_reach_the_command(
    #[case] arguments: &[&str],
    #[case] diagnostic: &str,
) -> Result<()> {
    let root = TempDir::new().context("create CLI argument fixture root")?;
    write_fixture(&root, "empty.toml", "")?;
    let config_path = fixture_path(&root, "empty.toml")?;
    let output = run_mxd(
        arguments,
        &[("MXD_CONFIG_PATH", config_path.as_str())],
        Some(root.path()),
    )?;

    assert_failure_contract(&output, 1, &[diagnostic])
}

/// Clap rejects global options after the subcommand and a separator before it.
#[rstest]
#[case::separator_before_command(&["--", "create-user"], "create-user")]
#[case::global_option_after_subcommand(
    &["create-user", "--database", "create-user"],
    "--database"
)]
#[case::credential_long_option(&["create-user", "--username", "alice"], "--username")]
fn unsupported_argument_placement_is_rejected(
    #[case] arguments: &[&str],
    #[case] diagnostic: &str,
) -> Result<()> {
    let root = TempDir::new().context("create CLI separator fixture root")?;
    let output = run_mxd(arguments, &[], Some(root.path()))?;

    anyhow::ensure!(
        output.status.code() == Some(2),
        "expected clap to reject argument placement, got {:?}",
        output.status.code()
    );
    anyhow::ensure!(output.stdout.is_empty(), "rejected CLI writes to stdout");
    anyhow::ensure!(
        String::from_utf8_lossy(&output.stderr).contains(diagnostic),
        "missing clap diagnostic for {diagnostic}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
