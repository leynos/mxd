//! Config-gated validators for flows that are still being implemented.
//!
//! These tests stay dormant by default so the main branch does not fail for
//! functionality that has not landed yet. Parallel feature branches can enable
//! individual validators through `validator/validator.toml` or environment
//! variables to require the missing flow before merge.

use rstest::rstest;
use test_util::AnyError;
use validator::{
    PendingValidator,
    ValidatorConfig,
    pending_validator_disabled_message,
    pending_validator_unimplemented_message,
};

#[rstest]
#[case::chat(PendingValidator::Chat)]
#[case::file_download(PendingValidator::FileDownload)]
fn pending_validator_requires_opt_in(#[case] validator: PendingValidator) -> Result<(), AnyError> {
    let config = ValidatorConfig::load()?;
    if !config.is_enabled(validator) {
        report_skip(&pending_validator_disabled_message(validator));
        return Ok(());
    }

    Err(AnyError::msg(pending_validator_unimplemented_message(
        validator,
    )))
}

fn report_skip(message: &str) {
    #[expect(
        clippy::print_stderr,
        reason = "skip message: inform user why test is being skipped"
    )]
    {
        eprintln!("{message}");
    }
}
