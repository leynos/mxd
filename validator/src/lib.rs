//! Integration test support for the `hx` client validator harness.
//!
//! The validator crate hosts end-to-end compatibility checks that drive the
//! `SynHX` `hx` client against `mxd-wireframe-server`. It also exposes a small
//! configuration surface for selectively enabling validators for flows that are
//! still being implemented on parallel branches.

mod config;

pub use config::{
    PendingValidator,
    VALIDATOR_CONFIG_ENV_VAR,
    VALIDATOR_ENABLE_CHAT_ENV_VAR,
    VALIDATOR_ENABLE_FILE_DOWNLOAD_ENV_VAR,
    ValidatorConfig,
    ValidatorConfigError,
    pending_validator_disabled_message,
    pending_validator_unimplemented_message,
};
