//! Umbrella integration test crate.

#![cfg(feature = "test-support")]
#![allow(
    unfulfilled_lint_expectations,
    reason = "test lint expectations may not all trigger"
)]
#![expect(clippy::tests_outside_test_module, reason = "umbrella test module")]
#![expect(clippy::panic_in_result_fn, reason = "test assertions")]
#![expect(clippy::print_stderr, reason = "test diagnostics")]
#![expect(clippy::str_to_string, reason = "test code")]
#[cfg(feature = "postgres")]
#[path = "integration/admin_postgres.rs"]
mod admin_postgres;

#[cfg(feature = "legacy-networking")]
#[path = "integration/server_legacy.rs"]
mod server_legacy;
