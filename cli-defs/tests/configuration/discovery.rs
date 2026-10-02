//! Discovery partitions: absent optional files, recorded failures, and required selectors.

use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use ortho_config::{ConfigDiscovery, MapEnv, serde_json::json};
use rstest::rstest;

use super::{
    contains_file_error,
    fixture,
    support::{ConfigFixture, defaults, file_values},
};

fn injected_discovery(isolated: &ConfigFixture) -> ortho_config::ConfigDiscoveryBuilder {
    let environment = MapEnv::new()
        .with_var("XDG_CONFIG_HOME", isolated.path("xdg"))
        .with_var("XDG_CONFIG_DIRS", isolated.path("xdg-system"));
    ConfigDiscovery::builder("mxd")
        .env_source(Arc::new(environment))
        .project_roots([isolated.path("project")])
}

#[rstest]
fn absent_optional_file_is_not_a_discovery_error(fixture: Result<ConfigFixture>) -> Result<()> {
    let isolated = fixture?;
    let discovery = injected_discovery(&isolated)
        .add_explicit_path(isolated.path("absent.toml"))
        .build();
    ensure!(
        discovery.load_first()?.is_none(),
        "configuration contract failed"
    );
    Ok(())
}

#[rstest]
#[case::malformed("database = [broken")]
#[case::missing_inheritance("extends = 'absent-parent.toml'")]
fn all_failed_candidates_report_typed_file_errors(
    fixture: Result<ConfigFixture>,
    #[case] contents: &str,
) -> Result<()> {
    let isolated = fixture?;
    isolated.directory.write("broken.toml", contents)?;
    let error = injected_discovery(&isolated)
        .add_explicit_path(isolated.path("broken.toml"))
        .build()
        .load_first()
        .err()
        .context("recorded failure cannot mean absence")?;
    ensure!(contains_file_error(&error), "configuration contract failed");
    isolated.probe(
        &["mxd", "--config-path", "broken.toml"],
        &[],
        &json!("file-error"),
    )
}

#[rstest]
fn later_successful_optional_candidate_can_replace_a_failed_candidate(
    fixture: Result<ConfigFixture>,
) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write("broken.toml", "database = [broken")?;
    isolated
        .directory
        .write("working.toml", include_str!("../fixtures/config.toml"))?;
    let loaded = injected_discovery(&isolated)
        .add_explicit_path(isolated.path("broken.toml"))
        .add_explicit_path(isolated.path("working.toml"))
        .build()
        .load_first()?
        .context("later valid optional candidate")?;
    ensure!(
        loaded.extract_inner::<String>("database")? == "file.db",
        "configuration values differ"
    );
    Ok(())
}

#[rstest]
fn generated_loader_also_accepts_a_later_valid_optional_file(
    fixture: Result<ConfigFixture>,
) -> Result<()> {
    let isolated = fixture?;
    isolated.directory.create_dir("separate-home")?;
    isolated
        .directory
        .write("separate-home/.mxd.toml", "database = [broken")?;
    isolated
        .directory
        .write(".mxd.toml", include_str!("../fixtures/config.toml"))?;
    isolated.probe(
        &["mxd"],
        &[("HOME", &isolated.path("separate-home"))],
        &file_values(),
    )
}

#[rstest]
#[case::cli_selector(true)]
#[case::environment_selector(false)]
fn cli_selector_is_required_while_environment_selector_allows_fallback(
    fixture: Result<ConfigFixture>,
    #[case] use_cli: bool,
) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write(".mxd.toml", include_str!("../fixtures/config.toml"))?;
    if use_cli {
        isolated.probe(
            &["mxd", "--config-path", "absent.toml"],
            &[],
            &json!("file-error"),
        )
    } else {
        isolated.probe(
            &["mxd"],
            &[("MXD_CONFIG_PATH", &isolated.path("absent.toml"))],
            &file_values(),
        )
    }
}

#[rstest]
fn selector_injection_is_confined_to_discovery_inputs(
    fixture: Result<ConfigFixture>,
) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write("chosen.toml", "database = 'chosen.db'")?;
    let environment = MapEnv::new().with_var("MXD_CONFIG_PATH", isolated.path("chosen.toml"));
    let discovery = ConfigDiscovery::builder("mxd")
        .env_var("MXD_CONFIG_PATH")
        .env_source(Arc::new(environment))
        .project_roots([isolated.path("project")])
        .build();
    let figure = discovery
        .load_first()?
        .context("injected selector is found")?;
    ensure!(
        figure.extract_inner::<String>("database")? == "chosen.db",
        "configuration values differ"
    );
    // The generated configuration-value loader is exercised with child-local
    // environments elsewhere; MapEnv does not inject its MXD_* value layer.
    isolated.probe(&["mxd"], &[], &defaults())
}

#[cfg(unix)]
#[rstest]
fn unreadable_file_is_a_failure_not_absence(fixture: Result<ConfigFixture>) -> Result<()> {
    use cap_std::fs::{Permissions, PermissionsExt};
    let isolated = fixture?;
    isolated
        .directory
        .write("unreadable.toml", "database = 'unreadable.db'")?;
    isolated
        .directory
        .set_permissions("unreadable.toml", Permissions::from_mode(0))?;
    let outcome = injected_discovery(&isolated)
        .add_explicit_path(isolated.path("unreadable.toml"))
        .build()
        .load_first();
    isolated
        .directory
        .set_permissions("unreadable.toml", Permissions::from_mode(0o600))?;
    let error = outcome.err().context("unreadable configuration")?;
    ensure!(
        contains_file_error(error.as_ref()),
        "configuration contract failed"
    );
    Ok(())
}

#[rstest]
fn failed_environment_selector_without_fallback_reports_error(
    fixture: Result<ConfigFixture>,
) -> Result<()> {
    let isolated = fixture?;
    isolated
        .directory
        .write("broken.toml", "database = [broken")?;
    isolated.probe(
        &["mxd"],
        &[("MXD_CONFIG_PATH", &isolated.path("broken.toml"))],
        &json!("file-error"),
    )
}
