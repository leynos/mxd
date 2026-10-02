//! Equivalent fixtures and the shipped YAML 1.2 provider contract.

use anyhow::Result;
#[cfg(feature = "yaml")]
use anyhow::{Context, ensure};
use ortho_config::serde_json::json;
use rstest::rstest;

use super::{
    fixture,
    support::{ConfigFixture, file_values},
};

#[rstest]
#[case::toml("config.toml", include_str!("../fixtures/config.toml"))]
#[cfg_attr(feature = "json5", case::json5("config.json5", include_str!("../fixtures/config.json5")))]
#[cfg_attr(feature = "yaml", case::yaml("config.yaml", include_str!("../fixtures/config.yaml")))]
fn equivalent_configuration_formats(
    fixture: Result<ConfigFixture>,
    #[case] filename: &str,
    #[case] contents: &str,
) -> Result<()> {
    let isolated = fixture?;
    isolated.directory.write(filename, contents)?;
    isolated.probe(&["mxd", "--config-path", filename], &[], &file_values())
}

#[rstest]
#[case::json5("config.json5", include_str!("../fixtures/config.json5"), cfg!(feature = "json5"))]
#[case::yaml("config.yaml", include_str!("../fixtures/config.yaml"), cfg!(feature = "yaml"))]
fn formats_are_disabled_only_when_the_dependency_graph_disables_them(
    fixture: Result<ConfigFixture>,
    #[case] filename: &str,
    #[case] contents: &str,
    #[case] enabled: bool,
) -> Result<()> {
    // Enabled formats are exercised by the equivalent fixture cases above.
    if enabled {
        return Ok(());
    }
    let isolated = fixture?;
    isolated.directory.write(filename, contents)?;
    isolated.probe(
        &["mxd", "--config-path", filename],
        &[],
        &json!("file-error"),
    )
}

#[cfg(feature = "yaml")]
#[rstest]
fn yaml_uses_strict_booleans_and_preserves_yaml_1_1_words(
    fixture: Result<ConfigFixture>,
) -> Result<()> {
    let isolated = fixture?;
    isolated.directory.write(
        "scalars.yaml",
        "yes_word: yes\nno_word: no\non_word: on\noff_word: off\ntruth: true\nfalsity: false\n",
    )?;
    let figure =
        ortho_config::load_config_file(std::path::Path::new(&isolated.path("scalars.yaml")))?
            .context("scalar fixture exists")?;
    let values: ortho_config::serde_json::Value = figure.extract()?;
    ensure!(
        values
            == json!({"yes_word":"yes", "no_word":"no", "on_word":"on", "off_word":"off", "truth":true, "falsity":false}),
        "configuration values differ"
    );
    Ok(())
}

#[cfg(feature = "yaml")]
#[rstest]
fn yaml_duplicate_mapping_keys_fail(fixture: Result<ConfigFixture>) -> Result<()> {
    let isolated = fixture?;
    isolated.directory.write(
        "duplicate.yaml",
        "database: first.db\ndatabase: second.db\n",
    )?;
    isolated.probe(
        &["mxd", "--config-path", "duplicate.yaml"],
        &[],
        &json!("provider-error"),
    )
}
