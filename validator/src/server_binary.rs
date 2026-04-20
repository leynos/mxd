//! Explicit wireframe server binary selection for validator runs.
//!
//! The validator harness should exercise a prebuilt `mxd-wireframe-server`
//! binary instead of paying an implicit `cargo run` compile cost during test
//! startup.

use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use thiserror::Error;

/// Environment variable that explicitly points at the wireframe server binary
/// for validator runs.
pub const VALIDATOR_SERVER_BINARY_ENV_VAR: &str = "MXD_VALIDATOR_SERVER_BINARY";

const SERVER_BINARY_ENV: &str = "CARGO_BIN_EXE_mxd-wireframe-server";
const SERVER_BINARY_NAME: &str = "mxd-wireframe-server";

/// Backend that the validator harness is running against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatorBackend {
    /// SQLite-backed validator run.
    Sqlite,
    /// PostgreSQL-backed validator run.
    Postgres,
}

impl ValidatorBackend {
    /// Detect the current backend from the validator crate feature set.
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(feature = "postgres")]
        {
            Self::Postgres
        }

        #[cfg(not(feature = "postgres"))]
        {
            Self::Sqlite
        }
    }
}

/// Error raised while resolving the explicit wireframe server binary.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ServerBinaryError {
    /// An explicitly requested server binary path does not exist.
    #[error("validator server binary does not exist: {path}")]
    MissingExplicitBinary {
        /// Environment variable that provided the path.
        env_var: &'static str,
        /// Requested path.
        path: PathBuf,
    },
    /// No known binary location contained a prebuilt wireframe server.
    #[error(
        "unable to find a prebuilt mxd-wireframe-server binary; checked {candidates:?}. Set \
         {env_var} to an explicit path"
    )]
    MissingDiscoveredBinary {
        /// Environment variable available for overriding the binary path.
        env_var: &'static str,
        /// Paths that were searched.
        candidates: Vec<PathBuf>,
    },
}

/// Resolve the prebuilt `mxd-wireframe-server` binary for the current backend.
///
/// Resolution order is:
///
/// 1. `MXD_VALIDATOR_SERVER_BINARY`
/// 2. existing `CARGO_BIN_EXE_mxd-wireframe-server`
/// 3. conventional backend-specific target paths under the workspace root
///
/// # Errors
///
/// Returns an error if an explicit override points at a missing file or if no
/// candidate path exists.
pub fn resolve_wireframe_server_binary() -> Result<PathBuf, ServerBinaryError> {
    let workspace_root = default_workspace_root();
    resolve_with_env(
        env::var_os(VALIDATOR_SERVER_BINARY_ENV_VAR).as_deref(),
        env::var_os(SERVER_BINARY_ENV).as_deref(),
        ValidatorBackend::current(),
        workspace_root.as_path(),
    )
}

fn resolve_with_env(
    override_binary: Option<&OsStr>,
    existing_binary: Option<&OsStr>,
    backend: ValidatorBackend,
    workspace_root: &Path,
) -> Result<PathBuf, ServerBinaryError> {
    if let Some(path) = override_binary.map(PathBuf::from) {
        return explicit_binary(path, VALIDATOR_SERVER_BINARY_ENV_VAR);
    }
    if let Some(path) = existing_binary.map(PathBuf::from) {
        return explicit_binary(path, SERVER_BINARY_ENV);
    }

    let candidates = candidate_binary_paths(workspace_root, backend);
    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .ok_or(ServerBinaryError::MissingDiscoveredBinary {
            env_var: VALIDATOR_SERVER_BINARY_ENV_VAR,
            candidates,
        })
}

fn explicit_binary(path: PathBuf, env_var: &'static str) -> Result<PathBuf, ServerBinaryError> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(ServerBinaryError::MissingExplicitBinary { env_var, path })
    }
}

fn default_workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from(".."), Path::to_path_buf)
}

fn candidate_binary_paths(workspace_root: &Path, backend: ValidatorBackend) -> Vec<PathBuf> {
    match backend {
        ValidatorBackend::Sqlite => vec![
            workspace_root
                .join("target")
                .join("debug")
                .join(SERVER_BINARY_NAME),
            workspace_root
                .join("target")
                .join("release")
                .join(SERVER_BINARY_NAME),
        ],
        ValidatorBackend::Postgres => vec![
            workspace_root
                .join("target")
                .join("postgres")
                .join("debug")
                .join(SERVER_BINARY_NAME),
            workspace_root
                .join("target")
                .join("postgres")
                .join("release")
                .join(SERVER_BINARY_NAME),
            workspace_root
                .join("target")
                .join("debug")
                .join(SERVER_BINARY_NAME),
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn touch_binary(path: &Path) {
        let parent = path.parent().expect("binary path should have a parent");
        fs::create_dir_all(parent).expect("create binary directory");
        fs::write(path, b"binary").expect("write binary");
    }

    #[test]
    fn explicit_override_wins_when_present() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let binary = temp_dir.path().join("custom-wireframe-server");
        touch_binary(&binary);

        let resolved = resolve_with_env(
            Some(binary.as_os_str()),
            None,
            ValidatorBackend::Sqlite,
            temp_dir.path(),
        )
        .expect("resolve binary");

        assert_eq!(resolved, binary);
    }

    #[test]
    fn missing_explicit_override_is_reported() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let binary = temp_dir.path().join("missing-wireframe-server");

        let err = resolve_with_env(
            Some(binary.as_os_str()),
            None,
            ValidatorBackend::Sqlite,
            temp_dir.path(),
        )
        .expect_err("missing explicit binary should fail");

        assert!(matches!(
            err,
            ServerBinaryError::MissingExplicitBinary {
                env_var: VALIDATOR_SERVER_BINARY_ENV_VAR,
                ..
            }
        ));
    }

    #[test]
    fn sqlite_resolution_uses_default_target_path() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let binary = temp_dir
            .path()
            .join("target")
            .join("debug")
            .join(SERVER_BINARY_NAME);
        touch_binary(&binary);

        let resolved = resolve_with_env(None, None, ValidatorBackend::Sqlite, temp_dir.path())
            .expect("resolve sqlite binary from default target path");

        assert_eq!(resolved, binary);
    }

    #[test]
    fn postgres_resolution_prefers_postgres_target_dir() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let postgres_binary = temp_dir
            .path()
            .join("target")
            .join("postgres")
            .join("debug")
            .join(SERVER_BINARY_NAME);
        let fallback_binary = temp_dir
            .path()
            .join("target")
            .join("debug")
            .join(SERVER_BINARY_NAME);
        touch_binary(&postgres_binary);
        touch_binary(&fallback_binary);

        let resolved = resolve_with_env(None, None, ValidatorBackend::Postgres, temp_dir.path())
            .expect("resolve postgres binary from backend-specific target dir");

        assert_eq!(resolved, postgres_binary);
    }
}
