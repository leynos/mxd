//! Configuration for validator feature selection.
//!
//! Implemented validator flows such as login and XOR/news validation always
//! run. Pending flows can be enabled selectively from a configuration file or
//! environment variables so parallel feature branches can opt into the checks
//! before the main branch can satisfy them.

use std::{
    env,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

/// Environment variable that points at an alternate validator config file.
pub const VALIDATOR_CONFIG_ENV_VAR: &str = "MXD_VALIDATOR_CONFIG";
/// Environment variable that enables the pending chat validator.
pub const VALIDATOR_ENABLE_CHAT_ENV_VAR: &str = "MXD_VALIDATOR_ENABLE_CHAT";
/// Environment variable that enables the pending file-download validator.
pub const VALIDATOR_ENABLE_FILE_DOWNLOAD_ENV_VAR: &str = "MXD_VALIDATOR_ENABLE_FILE_DOWNLOAD";

/// A pending validator that can be enabled selectively while the underlying
/// server functionality is still landing on a parallel branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingValidator {
    /// Validator for the chat flow.
    Chat,
    /// Validator for the file-download flow.
    FileDownload,
}

impl PendingValidator {
    const fn config_key(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::FileDownload => "file_download",
        }
    }

    const fn env_var(self) -> &'static str {
        match self {
            Self::Chat => VALIDATOR_ENABLE_CHAT_ENV_VAR,
            Self::FileDownload => VALIDATOR_ENABLE_FILE_DOWNLOAD_ENV_VAR,
        }
    }
}

impl std::fmt::Display for PendingValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.config_key())
    }
}

/// Effective validator configuration after merging file and environment inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValidatorConfig {
    chat: bool,
    file_download: bool,
}

impl ValidatorConfig {
    /// Load validator configuration from the default file and environment.
    ///
    /// The default file is `validator/validator.toml` relative to this crate's
    /// manifest directory. `MXD_VALIDATOR_CONFIG` can point at an alternate
    /// file, and environment toggles override file values.
    ///
    /// # Errors
    ///
    /// Returns an error when an explicitly requested config file does not
    /// exist, when a config file cannot be read or parsed, or when an
    /// environment override is not a valid boolean.
    pub fn load() -> Result<Self, ValidatorConfigError> { Self::load_with_env(&RealEnv) }

    /// Return `true` when the given pending validator is enabled.
    #[must_use]
    pub const fn is_enabled(self, validator: PendingValidator) -> bool {
        match validator {
            PendingValidator::Chat => self.chat,
            PendingValidator::FileDownload => self.file_download,
        }
    }

    fn load_with_env(env_source: &impl EnvSource) -> Result<Self, ValidatorConfigError> {
        let override_path = env_source.var(VALIDATOR_CONFIG_ENV_VAR).map(PathBuf::from);
        Self::load_from_sources(override_path.as_deref(), env_source)
    }

    fn load_from_sources(
        override_path: Option<&Path>,
        env_source: &impl EnvSource,
    ) -> Result<Self, ValidatorConfigError> {
        let default_path = Self::default_config_path();
        let config_path = override_path.unwrap_or(default_path.as_path());
        let mut config = if config_path.exists() {
            Self::from_file(config_path)?
        } else if override_path.is_some() {
            return Err(ValidatorConfigError::MissingConfigFile(
                config_path.to_path_buf(),
            ));
        } else {
            Self::default()
        };
        config.apply_env(env_source)?;
        Ok(config)
    }

    fn default_config_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("validator.toml")
    }

    fn from_file(path: &Path) -> Result<Self, ValidatorConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| ValidatorConfigError::ReadConfig {
                path: path.to_path_buf(),
                source,
            })?;
        let parsed: FileConfig =
            toml::from_str(&contents).map_err(|source| ValidatorConfigError::ParseConfig {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(Self {
            chat: parsed.validators.chat.unwrap_or(false),
            file_download: parsed.validators.file_download.unwrap_or(false),
        })
    }

    fn apply_env(&mut self, env_source: &impl EnvSource) -> Result<(), ValidatorConfigError> {
        if let Some(value) = env_source.var(VALIDATOR_ENABLE_CHAT_ENV_VAR) {
            self.chat = parse_bool_env(VALIDATOR_ENABLE_CHAT_ENV_VAR, &value)?;
        }
        if let Some(value) = env_source.var(VALIDATOR_ENABLE_FILE_DOWNLOAD_ENV_VAR) {
            self.file_download = parse_bool_env(VALIDATOR_ENABLE_FILE_DOWNLOAD_ENV_VAR, &value)?;
        }
        Ok(())
    }
}

/// An error raised while loading validator configuration.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ValidatorConfigError {
    /// The explicitly requested config file does not exist.
    #[error("validator config file does not exist: {0}")]
    MissingConfigFile(PathBuf),
    /// The config file could not be read.
    #[error("failed to read validator config {path}: {source}")]
    ReadConfig {
        /// Path that was being read.
        path: PathBuf,
        /// Underlying read error.
        source: std::io::Error,
    },
    /// The config file could not be parsed as TOML.
    #[error("failed to parse validator config {path}: {source}")]
    ParseConfig {
        /// Path that was being parsed.
        path: PathBuf,
        /// Underlying TOML parse error.
        source: toml::de::Error,
    },
    /// An environment override was not a valid boolean string.
    #[error("environment variable {env_var} must be 'true' or 'false', got '{value}'")]
    InvalidBool {
        /// Environment variable name.
        env_var: &'static str,
        /// Invalid provided value.
        value: String,
    },
}

/// Return the skip message emitted when a pending validator is disabled.
#[must_use]
pub fn pending_validator_disabled_message(validator: PendingValidator) -> String {
    format!(
        "pending validator '{validator}' is disabled; enable it with {} or {}",
        validator.env_var(),
        VALIDATOR_CONFIG_ENV_VAR
    )
}

/// Return the failure message emitted when an enabled pending validator has not
/// yet been implemented.
#[must_use]
pub fn pending_validator_unimplemented_message(validator: PendingValidator) -> String {
    format!(
        "pending validator '{validator}' is enabled, but the wireframe server flow is not \
         implemented yet"
    )
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    validators: FileValidators,
}

#[derive(Debug, Default, Deserialize)]
struct FileValidators {
    chat: Option<bool>,
    file_download: Option<bool>,
}

trait EnvSource {
    fn var(&self, key: &str) -> Option<String>;
}

struct RealEnv;

impl EnvSource for RealEnv {
    fn var(&self, key: &str) -> Option<String> { env::var(key).ok() }
}

fn parse_bool_env(env_var: &'static str, value: &str) -> Result<bool, ValidatorConfigError> {
    value
        .parse::<bool>()
        .map_err(|_| ValidatorConfigError::InvalidBool {
            env_var,
            value: value.to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rstest::{fixture, rstest};
    use tempfile::TempDir;

    use super::*;

    #[derive(Debug, Default, Clone)]
    struct FakeEnv {
        values: BTreeMap<String, String>,
    }

    #[derive(Debug)]
    struct EnablementCase {
        config_contents: Option<&'static str>,
        env: FakeEnv,
        validator: PendingValidator,
        expected_enabled: bool,
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

    fn config_path(temp_dir: &TempDir) -> PathBuf { temp_dir.path().join("validator.toml") }

    fn write_config(temp_dir: &TempDir, contents: &str) -> PathBuf {
        let path = config_path(temp_dir);
        fs::write(&path, contents).expect("write validator config");
        path
    }

    #[fixture]
    fn temp_config_dir() -> TempDir {
        match TempDir::new() {
            Ok(temp_dir) => temp_dir,
            Err(error) => panic!("create temp dir: {error}"),
        }
    }

    #[rstest]
    #[case::defaults_chat(EnablementCase {
        config_contents: None,
        env: FakeEnv::default(),
        validator: PendingValidator::Chat,
        expected_enabled: false,
    })]
    #[case::defaults_file_download(EnablementCase {
        config_contents: None,
        env: FakeEnv::default(),
        validator: PendingValidator::FileDownload,
        expected_enabled: false,
    })]
    #[case::file_config_chat(EnablementCase {
        config_contents: Some("[validators]\nchat = true\nfile_download = false\n"),
        env: FakeEnv::default(),
        validator: PendingValidator::Chat,
        expected_enabled: true,
    })]
    #[case::file_config_file_download(EnablementCase {
        config_contents: Some("[validators]\nchat = true\nfile_download = false\n"),
        env: FakeEnv::default(),
        validator: PendingValidator::FileDownload,
        expected_enabled: false,
    })]
    #[case::env_override_chat(EnablementCase {
        config_contents: Some("[validators]\nchat = false\nfile_download = false\n"),
        env: FakeEnv::default().with(VALIDATOR_ENABLE_CHAT_ENV_VAR, "true"),
        validator: PendingValidator::Chat,
        expected_enabled: true,
    })]
    #[case::env_override_file_download(EnablementCase {
        config_contents: Some("[validators]\nchat = false\nfile_download = false\n"),
        env: FakeEnv::default().with(VALIDATOR_ENABLE_FILE_DOWNLOAD_ENV_VAR, "true"),
        validator: PendingValidator::FileDownload,
        expected_enabled: true,
    })]
    fn validator_enablement_follows_sources(
        temp_config_dir: TempDir,
        #[case] case: EnablementCase,
    ) {
        let path = case
            .config_contents
            .map(|contents| write_config(&temp_config_dir, contents));

        let config =
            ValidatorConfig::load_from_sources(path.as_deref(), &case.env).expect("load config");

        assert_eq!(config.is_enabled(case.validator), case.expected_enabled);
    }

    #[test]
    fn invalid_env_bool_is_reported() {
        let env = FakeEnv::default().with(VALIDATOR_ENABLE_CHAT_ENV_VAR, "enabled");

        let err =
            ValidatorConfig::load_from_sources(None, &env).expect_err("invalid bool should fail");

        assert!(matches!(
            err,
            ValidatorConfigError::InvalidBool {
                env_var: VALIDATOR_ENABLE_CHAT_ENV_VAR,
                ..
            }
        ));
    }

    #[test]
    fn explicit_missing_config_file_fails() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let env = FakeEnv::default().with(
            VALIDATOR_CONFIG_ENV_VAR,
            &config_path(&temp_dir).display().to_string(),
        );

        let err = ValidatorConfig::load_with_env(&env).expect_err("missing file should fail");

        assert!(matches!(err, ValidatorConfigError::MissingConfigFile(_)));
    }
}
