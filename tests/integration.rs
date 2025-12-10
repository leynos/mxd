#![cfg(feature = "test-support")]
#![allow(clippy::tests_outside_test_module, reason = "umbrella test module")]
#![allow(clippy::panic_in_result_fn, reason = "test assertions")]
#![allow(clippy::print_stderr, reason = "test diagnostics")]
#![allow(clippy::str_to_string, reason = "test code")]

//! Umbrella integration test crate.
#[cfg(feature = "postgres")]
#[path = "integration/admin_postgres.rs"]
mod admin_postgres;

#[cfg(feature = "legacy-networking")]
#[path = "integration/server_legacy.rs"]
mod server_legacy;
