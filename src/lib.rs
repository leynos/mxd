//! Core library for the Marrakesh Express daemon.
//!
//! This crate exposes database utilities, protocol types, and helper
//! functions used by the server and supporting tools. Only one database
//! backend (either `sqlite` or `postgres`) should be enabled at a time.

#![cfg_attr(
    test,
    expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")
)]
#![cfg_attr(test, expect(clippy::str_to_string, reason = "test code"))]
#![cfg_attr(test, expect(clippy::unwrap_used, reason = "test code can panic"))]
#![cfg_attr(
    test,
    expect(clippy::indexing_slicing, reason = "test code with known bounds")
)]
#![cfg_attr(test, expect(clippy::shadow_reuse, reason = "test code shadowing"))]
#![cfg_attr(
    test,
    expect(clippy::let_underscore_must_use, reason = "test cleanup code")
)]
#![cfg_attr(
    test,
    expect(clippy::unneeded_field_pattern, reason = "test pattern matching")
)]

cfg_if::cfg_if! {
    if #[cfg(all(feature = "sqlite", feature = "postgres", not(feature = "lint")))] {
        compile_error!("Choose either sqlite or postgres, not both");
    } else if #[cfg(feature = "sqlite")] {
        pub use diesel::sqlite::Sqlite as DbBackend;
    } else if #[cfg(feature = "postgres")] {
        pub use diesel::pg::Pg as DbBackend;
    } else {
        compile_error!("Either the 'sqlite' or 'postgres' feature must be enabled");
    }
}

pub mod commands;
pub mod connection_flags;
pub mod db;
pub mod field_id;
pub mod handler;
pub mod header_util;
pub mod login;
pub mod models;
pub mod news_handlers;
pub(crate) mod news_path;
pub mod privileges;
pub mod protocol;
pub mod schema;
pub mod server;
pub mod transaction;
pub mod transaction_type;
pub mod users;
pub mod wireframe;
