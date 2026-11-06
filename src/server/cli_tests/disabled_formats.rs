//! Disabled format contracts for invocations with a verified feature graph.
//!
//! Verify the selected facade's Cargo features before running these tests:
//! dependency feature unification can enable a format independently of mxd's
//! feature flags.

use anyhow::{Context, Result, anyhow};
use ortho_config::OrthoError;
use rstest::rstest;
use tempfile::TempDir;

use super::{isolated_environment, load_config, select_file, write_config};

/// Assert a disabled format fails as a file error naming the selected path.
fn assert_format_disabled(name: &str, contents: &str, diagnostic: &str) -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let path = write_config(&root, name, contents)?;
    let directory = cap_std::fs::Dir::open_ambient_dir(root.path(), cap_std::ambient_authority())?;
    let canonical_path = directory.canonicalize(name)?;
    let error = load_config(["mxd"], select_file(isolated_environment(&root)?, &path))
        .expect_err("disabled configuration format must fail");
    let OrthoError::File {
        path: error_path,
        source,
    } = error.as_ref()
    else {
        return Err(anyhow!("expected file error, received {error}"));
    };
    anyhow::ensure!(
        error_path.is_absolute() && error_path.ends_with(&canonical_path),
        "expected selected path {canonical_path:?}, received {error_path:?}"
    );
    let io_error = source
        .downcast_ref::<std::io::Error>()
        .ok_or_else(|| anyhow!("expected an I/O error source"))?;
    anyhow::ensure!(
        io_error.kind() == std::io::ErrorKind::Other,
        "expected an Other I/O error, received {:?}",
        io_error.kind()
    );
    anyhow::ensure!(
        io_error.to_string().contains(diagnostic),
        "expected disabled-format diagnostic {diagnostic:?}, received {io_error}"
    );
    Ok(())
}

/// JSON and JSON5 files report the disabled `json5` feature.
#[cfg(not(feature = "json5"))]
#[rstest]
#[case::json("config.json", "{\"bind\": \"example\"}")]
#[case::json5("config.json5", "{bind: 'example'}")]
fn json_formats_report_disabled_feature(#[case] name: &str, #[case] contents: &str) -> Result<()> {
    assert_format_disabled(name, contents, "json5 feature disabled")
}

/// YAML files report the disabled `yaml` feature.
#[cfg(not(feature = "yaml"))]
#[rstest]
#[case::yaml("config.yaml")]
#[case::yml("config.yml")]
fn yaml_formats_report_disabled_feature(#[case] name: &str) -> Result<()> {
    assert_format_disabled(name, "bind: example\n", "yaml feature disabled")
}
