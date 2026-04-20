//! `SynHX` discovery and PTY helpers for validator runs.
//!
//! The validator harness must reject the unrelated Helix editor binary when it
//! is installed as `hx`, while still allowing local developer runs to skip
//! cleanly when `SynHX` is absent.

use std::{
    ffi::OsStr,
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use expectrl::{Regex, Session, spawn};
use thiserror::Error;
use wait_timeout::ChildExt;
use which::which;

/// Environment variable that overrides the `hx` binary used by the validator.
pub const VALIDATOR_HX_BINARY_ENV_VAR: &str = "MXD_VALIDATOR_HX_BINARY";

const HELIX_DETECTION_TIMEOUT: Duration = Duration::from_millis(500);

/// Error raised while resolving or launching the `hx` client.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HxClientError {
    /// The `hx` binary could not be found.
    #[error("hx binary not found; install SynHX or set MXD_VALIDATOR_HX_BINARY")]
    MissingBinary,
    /// The explicitly requested `hx` binary path does not exist.
    #[error("hx binary from {env_var} does not exist: {path}")]
    MissingExplicitBinary {
        /// Environment variable that provided the path.
        env_var: &'static str,
        /// Requested path.
        path: PathBuf,
    },
    /// The discovered `hx` binary appears to be the Helix editor.
    #[error("hx appears to be the Helix editor, not SynHX")]
    HelixBinary,
    /// Inspecting the `hx` binary failed before a session could be started.
    #[error("failed to inspect hx from {path}: {source}")]
    Probe {
        /// Binary path used for inspection.
        path: PathBuf,
        /// Underlying spawn error.
        source: std::io::Error,
    },
    /// Spawning the `hx` client failed.
    #[error("failed to spawn hx from {path}: {source}")]
    Spawn {
        /// Binary path used for spawning.
        path: PathBuf,
        /// Underlying spawn error.
        source: expectrl::Error,
    },
    /// The expected `SynHX` prompt did not appear.
    #[error("hx did not present the Hotline prompt: {0}")]
    Prompt(expectrl::Error),
    /// Best-effort cleanup failed.
    #[error("hx cleanup failed: {0}")]
    Cleanup(String),
}

/// Resolve the `SynHX` `hx` binary path and reject Helix if it is installed
/// under the same name.
///
/// # Errors
///
/// Returns an error if no `hx` binary can be found or if the resolved binary
/// is the Helix editor.
pub fn resolve_hx_binary() -> Result<PathBuf, HxClientError> {
    resolve_hx_binary_with_env(
        std::env::var_os(VALIDATOR_HX_BINARY_ENV_VAR).as_deref(),
        which("hx").ok(),
    )
}

fn resolve_hx_binary_with_env(
    override_path: Option<&OsStr>,
    discovered_path: Option<PathBuf>,
) -> Result<PathBuf, HxClientError> {
    let path = match override_path {
        Some(path) => explicit_hx_binary(PathBuf::from(path))?,
        None => discovered_path.ok_or(HxClientError::MissingBinary)?,
    };

    if hx_is_helix(&path)? {
        return Err(HxClientError::HelixBinary);
    }

    Ok(path)
}

fn explicit_hx_binary(path: PathBuf) -> Result<PathBuf, HxClientError> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(HxClientError::MissingExplicitBinary {
            env_var: VALIDATOR_HX_BINARY_ENV_VAR,
            path,
        })
    }
}

/// Spawn a `SynHX` session with the provided expect timeout.
///
/// # Errors
///
/// Returns an error if the `hx` binary cannot be resolved or if the PTY
/// session cannot be started.
pub fn spawn_hx_session(expect_timeout: Duration) -> Result<Session, HxClientError> {
    let path = resolve_hx_binary()?;
    let mut session = spawn(path.to_string_lossy().as_ref())
        .map_err(|source| HxClientError::Spawn { path, source })?;
    session.set_expect_timeout(Some(expect_timeout));
    Ok(session)
}

/// Wait for the standard `SynHX` prompt.
///
/// # Errors
///
/// Returns an error if the prompt does not appear before the configured expect
/// timeout elapses.
pub fn expect_hotline_prompt(session: &mut Session) -> Result<(), HxClientError> {
    session
        .expect(Regex("HX"))
        .map(|_| ())
        .map_err(HxClientError::Prompt)
}

/// Close a `SynHX` session, ignoring whether the process already exited.
///
/// # Errors
///
/// Returns an error if the PTY process could not be terminated cleanly.
pub fn terminate_hx(session: &mut Session) -> Result<(), HxClientError> {
    session
        .get_process_mut()
        .exit(true)
        .map(|_| ())
        .map_err(|error| HxClientError::Cleanup(error.to_string()))
}

fn hx_is_helix(path: &PathBuf) -> Result<bool, HxClientError> {
    let mut child = Command::new(path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| HxClientError::Probe {
            path: path.clone(),
            source,
        })?;

    if let Ok(Some(_)) = child.wait_timeout(HELIX_DETECTION_TIMEOUT) {
        let stdout = read_stream(child.stdout.take());
        let stderr = read_stream(child.stderr.take());
        let combined = format!("{stdout}{stderr}");
        Ok(output_looks_like_helix(&combined))
    } else {
        terminate_child(&mut child);
        Ok(false)
    }
}

fn output_looks_like_helix(output: &str) -> bool { output.to_lowercase().contains("helix") }

fn terminate_child(child: &mut Child) {
    let _kill_result = child.kill();
    let _wait_result = child.wait();
}

fn read_stream<T: Read>(maybe_stream: Option<T>) -> String {
    let mut buffer = Vec::new();
    if let Some(mut stream) = maybe_stream {
        let _read_result = stream.read_to_end(&mut buffer);
    }
    String::from_utf8_lossy(&buffer).to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn touch_binary(path: &std::path::Path) {
        let parent = path.parent().expect("binary path should have a parent");
        fs::create_dir_all(parent).expect("create binary directory");
        fs::write(path, b"binary").expect("write binary");
    }

    #[test]
    fn helix_output_is_detected_case_insensitively() {
        assert!(output_looks_like_helix("Helix 24.03"));
        assert!(output_looks_like_helix("helix terminal editor"));
    }

    #[test]
    fn non_helix_output_is_not_rejected() {
        assert!(!output_looks_like_helix("hx version 0.1.48.1"));
        assert!(!output_looks_like_helix(
            "load: p: No such file or directory"
        ));
    }

    #[test]
    fn explicit_override_must_exist() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let missing = temp_dir.path().join("missing-hx");

        let error = resolve_hx_binary_with_env(Some(missing.as_os_str()), None)
            .expect_err("missing explicit hx binary must fail");

        assert!(matches!(
            error,
            HxClientError::MissingExplicitBinary {
                env_var: VALIDATOR_HX_BINARY_ENV_VAR,
                path,
            } if path == missing
        ));
    }

    #[test]
    fn explicit_override_wins_when_present() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let explicit = temp_dir.path().join("custom-hx");
        let discovered = temp_dir.path().join("discovered-hx");
        touch_binary(&explicit);
        touch_binary(&discovered);

        let resolved =
            resolve_hx_binary_with_env(Some(explicit.as_os_str()), Some(discovered.clone()))
                .expect("resolve explicit hx binary");

        assert_eq!(resolved, explicit);
    }
}
