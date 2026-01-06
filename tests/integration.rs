//! Umbrella integration test crate.

#![cfg(feature = "test-support")]
#[cfg(feature = "postgres")]
#[path = "integration/admin_postgres.rs"]
mod admin_postgres;

#[cfg(feature = "legacy-networking")]
#[path = "integration/server_legacy.rs"]
mod server_legacy;
