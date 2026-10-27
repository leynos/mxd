//! Tests that preserve configuration file error distinctions.

use anyhow::{Context, Result, anyhow};
use cap_std::{
    ambient_authority,
    fs::{Dir, Permissions, PermissionsExt},
};
use ortho_config::OrthoError;
use rstest::rstest;
use tempfile::TempDir;

use super::{isolated_environment, load_config, select_file, write_config};

#[rstest]
fn missing_selected_config_is_not_treated_as_absent() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let missing = root.path().join("missing.toml");
    let environment = select_file(isolated_environment(&root)?, &missing);
    let error = load_config(["mxd"], environment)
        .err()
        .ok_or_else(|| anyhow!("selected file must be required"))?;
    let OrthoError::File { source, .. } = error.as_ref() else {
        return Err(anyhow!("expected file error, received {error}"));
    };
    let io_error = source
        .downcast_ref::<std::io::Error>()
        .ok_or_else(|| anyhow!("expected an I/O error source"))?;
    anyhow::ensure!(
        io_error.kind() == std::io::ErrorKind::NotFound,
        "selected missing file error: expected NotFound, got {:?}",
        io_error.kind()
    );
    Ok(())
}

#[rstest]
fn malformed_config_remains_a_file_error() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let path = write_config(&root, "malformed.toml", "bind = [")?;
    let error = load_config(["mxd"], select_file(isolated_environment(&root)?, &path))
        .expect_err("malformed TOML must fail");
    let OrthoError::File { source, .. } = error.as_ref() else {
        return Err(anyhow!("expected malformed-file error, received {error}"));
    };
    anyhow::ensure!(
        source.downcast_ref::<std::io::Error>().is_none(),
        "parse errors remain distinct from filesystem errors"
    );
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn unreadable_config_remains_a_permission_error() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let path = write_config(&root, "unreadable.toml", "bind = '127.0.0.1:8000'")?;
    let directory = Dir::open_ambient_dir(root.path(), ambient_authority())
        .context("open isolated config root")?;
    directory
        .set_permissions("unreadable.toml", Permissions::from_mode(0o000))
        .context("remove configuration read permissions")?;
    match directory.open("unreadable.toml") {
        Ok(_) => {
            // Privileged runners can override mode bits, so this process
            // cannot use filesystem permissions to exercise the error path.
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {}
        Err(error) => return Err(error).context("verify configuration read permissions"),
    }
    let error = load_config(["mxd"], select_file(isolated_environment(&root)?, &path))
        .expect_err("unreadable TOML must fail");
    let OrthoError::File { source, .. } = error.as_ref() else {
        return Err(anyhow!("expected unreadable-file error, received {error}"));
    };
    let io_error = source
        .downcast_ref::<std::io::Error>()
        .ok_or_else(|| anyhow!("expected an I/O error source"))?;
    anyhow::ensure!(
        io_error.kind() == std::io::ErrorKind::PermissionDenied,
        "unreadable file error: expected PermissionDenied, got {:?}",
        io_error.kind()
    );
    Ok(())
}

#[rstest]
fn invalid_config_value_fails_instead_of_using_defaults() -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let path = write_config(&root, "invalid-value.toml", "argon2_m_cost = 'fast'")?;
    let error = load_config(["mxd"], select_file(isolated_environment(&root)?, &path))
        .err()
        .ok_or_else(|| anyhow!("invalid field type must fail"))?;
    anyhow::ensure!(
        matches!(
            error.as_ref(),
            OrthoError::Merge { .. } | OrthoError::Gathering(_) | OrthoError::Aggregate(_)
        ),
        "invalid values remain merge errors, received {error}"
    );
    Ok(())
}
