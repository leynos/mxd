//! High-level validator harness helpers.
//!
//! This module centralizes prerequisite checks, explicit wireframe server
//! targeting, and `SynHX` PTY setup so end-to-end validator tests can stay
//! focused on protocol flows rather than process orchestration.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use expectrl::{Regex, Session};
use test_util::{AnyError, TestServer, with_env_var};

use crate::{
    hx_client::{HxClientError, expect_hotline_prompt, spawn_hx_session, terminate_hx},
    policy::{PrerequisiteResolution, ValidatorRunPolicy},
    server_binary::{ServerBinaryError, resolve_wireframe_server_binary},
};

const SERVER_BINARY_ENV: &str = "CARGO_BIN_EXE_mxd-wireframe-server";
const DEFAULT_EXPECT_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_EXPECT_TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_MANIFEST_PATH: &str = "../Cargo.toml";

/// Prepared validator harness with explicit `hx` and server binary targeting.
#[derive(Debug, Clone)]
pub struct ValidatorHarness {
    server_binary: PathBuf,
}

impl ValidatorHarness {
    /// Prepare the validator harness or skip locally when prerequisites are
    /// unavailable.
    ///
    /// # Errors
    ///
    /// Returns an error when fail-closed policy is active and a prerequisite is
    /// missing or invalid.
    pub fn prepare() -> Result<Option<Self>, AnyError> {
        let policy = ValidatorRunPolicy::load()?;
        let server_binary = match resolve_wireframe_server_binary() {
            Ok(path) => path,
            Err(error) => return handle_prerequisite(policy, error),
        };
        if let Err(error) = crate::hx_client::resolve_hx_binary() {
            return handle_prerequisite(policy, error);
        }

        Ok(Some(Self { server_binary }))
    }

    /// Return the explicit wireframe server binary used by the harness.
    #[must_use]
    pub fn server_binary(&self) -> &Path { self.server_binary.as_path() }

    /// Start a test server with the shared wireframe binary environment set.
    ///
    /// # Errors
    ///
    /// Returns an error if database setup, server launch, or environment
    /// mutation fails.
    pub fn start_server_with_setup<F>(&self, setup: F) -> Result<TestServer, AnyError>
    where
        F: FnOnce(&str) -> Result<(), AnyError>,
    {
        let binary = self.server_binary.display().to_string();
        with_env_var(SERVER_BINARY_ENV, Some(&binary), || {
            TestServer::start_with_setup(DEFAULT_MANIFEST_PATH, |db| setup(db.as_str()))
        })
    }

    /// Spawn a `SynHX` session and wait for the standard prompt.
    ///
    /// # Errors
    ///
    /// Returns an error if the client cannot be launched or if the prompt does
    /// not appear.
    pub fn spawn_hx(&self) -> Result<Session, AnyError> {
        let mut session = spawn_hx_session(DEFAULT_EXPECT_TIMEOUT)?;
        expect_hotline_prompt(&mut session)?;
        Ok(session)
    }
}

/// Report a skipped validator test to stderr.
pub fn report_skip(message: &str) {
    #[expect(
        clippy::print_stderr,
        reason = "skip message: inform user why test is being skipped"
    )]
    {
        eprintln!("{message}");
    }
}

/// Send a command and assert that the session output matches the provided
/// regular expression.
///
/// # Errors
///
/// Returns an error if the write fails or if the expected output is not
/// observed before the PTY timeout expires.
pub fn send_line_and_expect(
    session: &mut Session,
    command: impl AsRef<str>,
    pattern: &'static str,
    context: &'static str,
) -> Result<(), AnyError> {
    session.send_line(command.as_ref())?;
    expect_output(session, pattern, context)
}

/// Assert that the current session output matches the provided regular
/// expression.
///
/// # Errors
///
/// Returns an error if the expected output is not observed before the PTY
/// timeout expires.
pub fn expect_output(
    session: &mut Session,
    pattern: &'static str,
    context: &'static str,
) -> Result<(), AnyError> {
    expect_output_with_timeout(session, pattern, context, DEFAULT_EXPECT_TIMEOUT)
}

/// Assert that the current session output matches the provided regular
/// expression, using a temporary expect timeout.
///
/// # Errors
///
/// Returns an error if the expected output is not observed before the PTY
/// timeout expires.
pub fn expect_output_with_timeout(
    session: &mut Session,
    pattern: &'static str,
    context: &'static str,
    timeout: Duration,
) -> Result<(), AnyError> {
    session.set_expect_timeout(Some(timeout));
    let result = session.expect(Regex(pattern)).map(|_| ()).map_err(|error| {
        AnyError::msg(format_expect_error(
            context,
            &error,
            pending_output(session),
        ))
    });
    session.set_expect_timeout(Some(DEFAULT_EXPECT_TIMEOUT));
    result
}

/// Assert that the provided pattern does not appear in the current session
/// output window.
///
/// # Errors
///
/// Returns an error if the pattern unexpectedly appears or if expect timeout
/// configuration fails.
pub fn expect_no_match(
    session: &mut Session,
    pattern: &'static str,
    context: &'static str,
) -> Result<(), AnyError> {
    session.set_expect_timeout(Some(Duration::from_millis(250)));
    let result = session.expect(Regex(pattern));
    session.set_expect_timeout(Some(DEFAULT_EXPECT_TIMEOUT));
    if result.is_ok() {
        return Err(AnyError::msg(context));
    }
    Ok(())
}

/// Close a `SynHX` session, reporting cleanup failures as skipped diagnostics.
pub fn close_hx(session: &mut Session) {
    if let Err(error) = session.send_line("/quit") {
        report_skip(&format!("hx quit command failed ({error})"));
    }
    if let Err(error) = terminate_hx(session) {
        report_skip(&error.to_string());
    }
}

/// Timeout used when waiting for `hx` to authenticate against a test server.
#[must_use]
pub const fn connect_expect_timeout() -> Duration { CONNECT_EXPECT_TIMEOUT }

fn pending_output(session: &mut Session) -> Option<String> {
    session
        .check(Regex("(?s).+"))
        .ok()
        .filter(|captures| !captures.is_empty())
        .map(|captures| String::from_utf8_lossy(captures.as_bytes()).into_owned())
}

fn format_expect_error(
    context: &str,
    error: &expectrl::Error,
    transcript: Option<String>,
) -> String {
    let Some(pending_transcript) = transcript.filter(|output| !output.trim().is_empty()) else {
        return format!("{context}: {error}");
    };
    format!(
        "{context}: {error}; pending output: {}",
        pending_transcript.escape_default()
    )
}

fn handle_prerequisite(
    policy: ValidatorRunPolicy,
    error: impl Into<PrerequisiteError>,
) -> Result<Option<ValidatorHarness>, AnyError> {
    match policy.prerequisite_resolution(error.into().to_string()) {
        PrerequisiteResolution::Skip(message) => {
            report_skip(&message);
            Ok(None)
        }
        PrerequisiteResolution::Fail(message) => Err(AnyError::msg(message)),
    }
}

#[derive(Debug)]
enum PrerequisiteError {
    Hx(HxClientError),
    ServerBinary(ServerBinaryError),
}

impl std::fmt::Display for PrerequisiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hx(error) => error.fmt(f),
            Self::ServerBinary(error) => error.fmt(f),
        }
    }
}

impl From<HxClientError> for PrerequisiteError {
    fn from(value: HxClientError) -> Self { Self::Hx(value) }
}

impl From<ServerBinaryError> for PrerequisiteError {
    fn from(value: ServerBinaryError) -> Self { Self::ServerBinary(value) }
}

#[cfg(test)]
mod tests {
    use super::format_expect_error;

    #[test]
    fn format_expect_error_omits_empty_transcript() {
        let message = format_expect_error(
            "context",
            &expectrl::Error::ExpectTimeout,
            Some(" \n\t".to_owned()),
        );

        assert_eq!(
            message,
            "context: Reached a timeout for expect type of command"
        );
    }

    #[test]
    fn format_expect_error_includes_transcript() {
        let message = format_expect_error(
            "context",
            &expectrl::Error::ExpectTimeout,
            Some("connected to host\nHX".to_owned()),
        );

        assert_eq!(
            message,
            concat!(
                "context: Reached a timeout for expect type of command; pending output: ",
                "connected to host\\nHX"
            )
        );
    }
}
