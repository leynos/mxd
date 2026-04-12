//! Execution policy for the validator harness.
//!
//! Local developer runs should stay informative when prerequisites such as
//! `hx` or a prebuilt server binary are absent. CI must fail closed instead of
//! silently skipping the validator path.

use std::env;

use thiserror::Error;

/// Environment variable that overrides whether validator prerequisites fail
/// closed.
pub const VALIDATOR_FAIL_CLOSED_ENV_VAR: &str = "MXD_VALIDATOR_FAIL_CLOSED";

/// Effective execution policy for validator prerequisite handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValidatorRunPolicy {
    fail_closed: bool,
}

impl ValidatorRunPolicy {
    /// Load the effective policy from the environment.
    ///
    /// `MXD_VALIDATOR_FAIL_CLOSED` takes precedence when set. Otherwise the
    /// presence of `CI=true` enables fail-closed behaviour.
    ///
    /// # Errors
    ///
    /// Returns an error if `MXD_VALIDATOR_FAIL_CLOSED` is present but is not a
    /// valid boolean string.
    pub fn load() -> Result<Self, ValidatorRunPolicyError> { Self::load_with_env(&RealEnv) }

    /// Return `true` when missing prerequisites should fail the run.
    #[must_use]
    pub const fn should_fail_closed(self) -> bool { self.fail_closed }

    /// Decide whether a prerequisite failure should skip or fail the test.
    #[must_use]
    pub fn prerequisite_resolution(
        self,
        prerequisite_message: impl Into<String>,
    ) -> PrerequisiteResolution {
        let message = prerequisite_message.into();
        if self.should_fail_closed() {
            PrerequisiteResolution::Fail(message)
        } else {
            PrerequisiteResolution::Skip(message)
        }
    }

    fn load_with_env(env_source: &impl EnvSource) -> Result<Self, ValidatorRunPolicyError> {
        if let Some(value) = env_source.var(VALIDATOR_FAIL_CLOSED_ENV_VAR) {
            return Ok(Self {
                fail_closed: parse_bool_env(VALIDATOR_FAIL_CLOSED_ENV_VAR, &value)?,
            });
        }

        Ok(Self {
            fail_closed: env_source.var("CI").as_deref() == Some("true"),
        })
    }
}

/// Result of evaluating how the harness should react to a missing prerequisite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrerequisiteResolution {
    /// Emit a skip message and return successfully.
    Skip(String),
    /// Fail the validator immediately.
    Fail(String),
}

/// Error raised when the execution policy environment is invalid.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidatorRunPolicyError {
    /// An environment variable contained an invalid boolean string.
    #[error("environment variable {env_var} must be 'true' or 'false', got '{value}'")]
    InvalidBool {
        /// Environment variable name.
        env_var: &'static str,
        /// Invalid provided value.
        value: String,
    },
}

trait EnvSource {
    fn var(&self, key: &str) -> Option<String>;
}

struct RealEnv;

impl EnvSource for RealEnv {
    fn var(&self, key: &str) -> Option<String> { env::var(key).ok() }
}

fn parse_bool_env(env_var: &'static str, value: &str) -> Result<bool, ValidatorRunPolicyError> {
    value
        .parse::<bool>()
        .map_err(|_| ValidatorRunPolicyError::InvalidBool {
            env_var,
            value: value.to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Debug, Default)]
    struct FakeEnv {
        values: BTreeMap<String, String>,
    }

    impl FakeEnv {
        fn with(mut self, key: &str, value: &str) -> Self {
            self.values.insert(key.to_owned(), value.to_owned());
            self
        }
    }

    impl EnvSource for FakeEnv {
        fn var(&self, key: &str) -> Option<String> { self.values.get(key).cloned() }
    }

    #[test]
    fn defaults_to_non_ci_skip_behaviour() {
        let policy = ValidatorRunPolicy::load_with_env(&FakeEnv::default()).expect("load policy");

        assert!(!policy.should_fail_closed());
    }

    #[test]
    fn ci_defaults_to_fail_closed() {
        let policy = ValidatorRunPolicy::load_with_env(&FakeEnv::default().with("CI", "true"))
            .expect("load policy");

        assert!(policy.should_fail_closed());
    }

    #[test]
    fn explicit_override_beats_ci_detection() {
        let policy = ValidatorRunPolicy::load_with_env(
            &FakeEnv::default()
                .with("CI", "true")
                .with(VALIDATOR_FAIL_CLOSED_ENV_VAR, "false"),
        )
        .expect("load policy");

        assert!(!policy.should_fail_closed());
    }

    #[test]
    fn invalid_override_is_reported() {
        let err = ValidatorRunPolicy::load_with_env(
            &FakeEnv::default().with(VALIDATOR_FAIL_CLOSED_ENV_VAR, "required"),
        )
        .expect_err("invalid override should fail");

        assert_eq!(
            err,
            ValidatorRunPolicyError::InvalidBool {
                env_var: VALIDATOR_FAIL_CLOSED_ENV_VAR,
                value: "required".to_owned(),
            }
        );
    }

    #[test]
    fn prerequisite_resolution_reflects_policy() {
        let skip = ValidatorRunPolicy::default().prerequisite_resolution("missing hx");
        let fail = ValidatorRunPolicy { fail_closed: true }.prerequisite_resolution("missing hx");

        assert_eq!(skip, PrerequisiteResolution::Skip("missing hx".to_owned()));
        assert_eq!(fail, PrerequisiteResolution::Fail("missing hx".to_owned()));
    }
}
