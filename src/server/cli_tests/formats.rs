//! Tests for optional configuration file formats.

use anyhow::Result;
#[cfg(feature = "yaml")]
use ortho_config::OrthoError;
use rstest::rstest;
use tempfile::TempDir;

use super::*;

#[cfg(feature = "json5")]
#[rstest]
fn json5_config_is_loaded_by_ortho_config() -> Result<()> {
    let root = TempDir::new()?;
    let path = write_config(&root, "config.json5", "{ bind: 'json5.example:5500' }")?;
    let cfg = load_config(["mxd"], select_file(isolated_environment(&root)?, &path))?;
    ensure_equal!(cfg.bind.as_str(), "json5.example:5500", "JSON5 bind")?;
    Ok(())
}

#[cfg(feature = "yaml")]
#[rstest]
#[case::yes("yes")]
#[case::no("no")]
#[case::on("on")]
#[case::off("off")]
fn yaml_uses_yaml_12_string_scalars(#[case] scalar: &str) -> Result<()> {
    let root = TempDir::new()?;
    let path = write_config(&root, "config.yaml", &format!("bind: {scalar}\n"))?;
    let cfg = load_config(["mxd"], select_file(isolated_environment(&root)?, &path))?;
    ensure_equal!(cfg.bind.as_str(), scalar, "YAML 1.2 string bind")?;
    Ok(())
}

#[cfg(feature = "yaml")]
#[rstest]
#[case::true_literal("true", true)]
#[case::false_literal("false", false)]
fn yaml_uses_yaml_12_boolean_scalars(#[case] scalar: &str, #[case] expected: bool) -> Result<()> {
    let root = TempDir::new()?;
    let path = write_config(&root, "config.yaml", &format!("bind: {scalar}\n"))?;
    let chain = ortho_config::load_config_file_as_chain(&path)?
        .ok_or_else(|| anyhow::anyhow!("selected YAML file was not loaded"))?;
    let (value, _) = chain
        .values
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("selected YAML file had no layer"))?;
    ensure_equal!(
        value.get("bind"),
        Some(&serde_json::Value::Bool(expected)),
        "YAML 1.2 boolean scalar",
    )?;
    Ok(())
}

#[cfg(feature = "yaml")]
#[rstest]
fn yaml_duplicate_keys_are_rejected() -> Result<()> {
    let root = TempDir::new()?;
    let path = write_config(&root, "duplicate.yaml", "bind: first\nbind: second\n")?;
    let error = load_config(["mxd"], select_file(isolated_environment(&root)?, &path))
        .err()
        .ok_or_else(|| anyhow::anyhow!("duplicate YAML keys must fail"))?;
    anyhow::ensure!(
        matches!(error.as_ref(), OrthoError::Merge { .. }),
        "YAML provider errors remain merge errors: {error}"
    );
    let diagnostic = error.to_string();
    anyhow::ensure!(
        diagnostic.contains("duplicate mapping key: bind"),
        "{diagnostic}"
    );
    anyhow::ensure!(diagnostic.contains("duplicate.yaml"), "{diagnostic}");
    Ok(())
}
