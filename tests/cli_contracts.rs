//! Process-level checks for mxd CLI output and exit contracts.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use cap_std::{ambient_authority, fs::Dir};
use rstest::rstest;
use tempfile::TempDir;

const GENERATED_MAN_PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/mxd.1"));

fn run_mxd(
    args: &[&str],
    environment: &[(&str, &str)],
    current_dir: Option<&Path>,
) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mxd"));
    command
        .env_clear()
        .env("LC_ALL", "C")
        .args(args)
        .envs(environment.iter().copied());
    if let Some(directory) = current_dir {
        command.current_dir(directory);
    }
    command.output()
}

fn write_fixture(root: &TempDir, name: &str, contents: &str) -> Result<()> {
    let directory = Dir::open_ambient_dir(root.path(), ambient_authority())
        .context("open CLI fixture directory")?;
    directory
        .write(name, contents.as_bytes())
        .context("write CLI fixture")
}

fn fixture_path(root: &TempDir, name: &str) -> Result<String> {
    root.path()
        .join(name)
        .into_os_string()
        .into_string()
        .map_err(|_| anyhow::anyhow!("temporary {name} path is not UTF-8"))
}

fn isolated_discovery_environment(root: &TempDir) -> Result<Vec<(String, String)>> {
    let directory = Dir::open_ambient_dir(root.path(), ambient_authority())
        .context("open isolated discovery root")?;
    for path in ["home", "xdg-config-home", "xdg-config-dirs"] {
        directory
            .create_dir(path)
            .with_context(|| format!("create isolated discovery directory {path}"))?;
    }

    [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "xdg-config-home"),
        ("XDG_CONFIG_DIRS", "xdg-config-dirs"),
    ]
    .into_iter()
    .map(|(key, name)| Ok((key.to_owned(), fixture_path(root, name)?)))
    .collect::<Result<Vec<_>>>()
}

fn assert_failure_contract(
    output: &std::process::Output,
    expected_status: i32,
    expected_diagnostics: &[&str],
) -> Result<()> {
    anyhow::ensure!(
        output.status.code() == Some(expected_status),
        "expected exit status {expected_status}, got {:?}",
        output.status.code()
    );
    anyhow::ensure!(output.stdout.is_empty(), "failed CLI writes to stdout");

    let stderr = String::from_utf8_lossy(&output.stderr);
    for diagnostic in expected_diagnostics {
        anyhow::ensure!(stderr.contains(diagnostic), "{stderr}");
    }

    Ok(())
}

#[rstest]
fn help_uses_stdout_and_exits_successfully() -> Result<()> {
    let output = run_mxd(&["--help"], &[], None)?;
    anyhow::ensure!(
        output.status.code() == Some(0),
        "help exits successfully, got {:?}",
        output.status.code()
    );
    anyhow::ensure!(output.stdout.windows(5).any(|w| w == b"Usage"));
    anyhow::ensure!(output.stdout.windows(4).any(|w| w == b"bind"));
    anyhow::ensure!(output.stderr.is_empty());
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum CliFailureScenario {
    UnknownSubcommand,
    InvalidValue,
    MissingEnvironmentConfig,
    MissingCliConfig,
    MalformedExplicitConfig,
    MalformedProjectDotfile,
}

struct CliFailureCase {
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    expected_status: i32,
    diagnostics: &'static [&'static str],
    current_dir: Option<PathBuf>,
}

impl CliFailureCase {
    fn new(
        arguments: &[&str],
        environment: Vec<(String, String)>,
        expected_status: i32,
        diagnostics: &'static [&'static str],
    ) -> Self {
        Self {
            arguments: arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect(),
            environment,
            expected_status,
            diagnostics,
            current_dir: None,
        }
    }

    fn in_directory(mut self, path: &Path) -> Self {
        self.current_dir = Some(path.to_path_buf());
        self
    }
}

fn build_failure_case(root: &TempDir, scenario: CliFailureScenario) -> Result<CliFailureCase> {
    let case = match scenario {
        CliFailureScenario::UnknownSubcommand => CliFailureCase::new(
            &["not-a-command"],
            Vec::new(),
            2,
            &["unrecognized subcommand"],
        ),
        CliFailureScenario::InvalidValue => CliFailureCase::new(
            &["--argon2-m-cost", "not-a-number"],
            Vec::new(),
            2,
            &["invalid value"],
        ),
        CliFailureScenario::MissingEnvironmentConfig => CliFailureCase::new(
            &["--bind", "127.0.0.1:5500"],
            vec![(
                "MXD_CONFIG_PATH".to_owned(),
                fixture_path(root, "missing.toml")?,
            )],
            1,
            &["load configuration", "missing.toml"],
        ),
        CliFailureScenario::MissingCliConfig => CliFailureCase::new(
            &[
                "--config-path",
                &fixture_path(root, "missing.toml")?,
                "--bind",
                "127.0.0.1:5500",
            ],
            Vec::new(),
            1,
            &["load configuration", "missing.toml"],
        ),
        CliFailureScenario::MalformedExplicitConfig => {
            write_fixture(root, "malformed.toml", "bind = [")?;
            CliFailureCase::new(
                &["--bind", "127.0.0.1:5500"],
                vec![(
                    "MXD_CONFIG_PATH".to_owned(),
                    fixture_path(root, "malformed.toml")?,
                )],
                1,
                &["load configuration", "malformed.toml"],
            )
        }
        CliFailureScenario::MalformedProjectDotfile => {
            write_fixture(root, ".mxd.toml", "bind = [")?;
            CliFailureCase::new(
                &["--bind", "127.0.0.1:5500"],
                isolated_discovery_environment(root)?,
                1,
                &["load configuration", ".mxd.toml"],
            )
            .in_directory(root.path())
        }
    };

    Ok(case)
}

#[rstest]
#[case::unknown_subcommand(CliFailureScenario::UnknownSubcommand)]
#[case::invalid_value(CliFailureScenario::InvalidValue)]
#[case::missing_environment_config(CliFailureScenario::MissingEnvironmentConfig)]
#[case::missing_cli_config(CliFailureScenario::MissingCliConfig)]
#[case::malformed_explicit_config(CliFailureScenario::MalformedExplicitConfig)]
#[case::malformed_project_dotfile(CliFailureScenario::MalformedProjectDotfile)]
fn cli_failure_scenarios_preserve_exit_and_stream_contracts(
    #[case] scenario: CliFailureScenario,
) -> Result<()> {
    let root = TempDir::new().context("create CLI failure fixture root")?;
    let case = build_failure_case(&root, scenario)?;
    let argument_refs = case
        .arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let environment_refs = case
        .environment
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let output = run_mxd(
        &argument_refs,
        &environment_refs,
        case.current_dir.as_deref(),
    )?;

    assert_failure_contract(&output, case.expected_status, case.diagnostics)
}

#[rstest]
fn generated_man_page_keeps_shared_cli_options_and_subcommands() {
    assert!(GENERATED_MAN_PAGE.contains(r"\-\-bind"));
    assert!(GENERATED_MAN_PAGE.contains(r"create\-user"));
}
