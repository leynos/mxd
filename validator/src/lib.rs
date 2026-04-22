//! Integration test support for the `hx` client validator harness.
//!
//! The validator crate hosts end-to-end compatibility checks that drive the
//! `SynHX` `hx` client against `mxd-wireframe-server`. It also exposes a small
//! configuration surface for selectively enabling validators for flows that are
//! still being implemented on parallel branches.

mod config;
mod harness;
mod hx_client;
mod policy;
mod server_binary;

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
pub use harness::{
    ValidatorHarness,
    close_hx,
    connect_expect_timeout,
    expect_no_match,
    expect_output,
    expect_output_with_timeout,
    report_skip,
    send_line_and_expect,
};
pub use hx_client::{HxClientError, VALIDATOR_HX_BINARY_ENV_VAR};
pub use policy::{
    PrerequisiteResolution,
    VALIDATOR_FAIL_CLOSED_ENV_VAR,
    ValidatorRunPolicy,
    ValidatorRunPolicyError,
};
pub use server_binary::{ServerBinaryError, VALIDATOR_SERVER_BINARY_ENV_VAR, ValidatorBackend};
