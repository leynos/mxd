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

    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;

    #[derive(Default)]
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

    fn config_path(temp_dir: &TempDir) -> PathBuf { temp_dir.path().join("validator.toml") }

    fn write_config(temp_dir: &TempDir, contents: &str) -> PathBuf {
        let path = config_path(temp_dir);
        fs::write(&path, contents).expect("write validator config");
        path
    }

    #[test]
    fn defaults_disable_pending_validators() {
        let config =
            ValidatorConfig::load_from_sources(None, &FakeEnv::default()).expect("load config");
        assert!(!config.is_enabled(PendingValidator::Chat));
        assert!(!config.is_enabled(PendingValidator::FileDownload));
    }

    #[test]
    fn file_config_enables_pending_validator() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let path = write_config(
            &temp_dir,
            "[validators]\nchat = true\nfile_download = false\n",
        );

        let config = ValidatorConfig::load_from_sources(Some(&path), &FakeEnv::default())
            .expect("load config");

        assert!(config.is_enabled(PendingValidator::Chat));
        assert!(!config.is_enabled(PendingValidator::FileDownload));
    }

    #[rstest]
    #[case::chat(VALIDATOR_ENABLE_CHAT_ENV_VAR, PendingValidator::Chat)]
    #[case::file_download(VALIDATOR_ENABLE_FILE_DOWNLOAD_ENV_VAR, PendingValidator::FileDownload)]
    fn env_override_enables_pending_validator(
        #[case] env_var: &'static str,
        #[case] validator: PendingValidator,
    ) {
        let temp_dir = TempDir::new().expect("create temp dir");
        let path = write_config(
            &temp_dir,
            "[validators]\nchat = false\nfile_download = false\n",
        );
        let env = FakeEnv::default().with(env_var, "true");

        let config = ValidatorConfig::load_from_sources(Some(&path), &env).expect("load config");

        assert!(config.is_enabled(validator));
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
