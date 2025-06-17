//! Core library for the Marrakesh Express daemon.
//!
//! This crate exposes database utilities, protocol types, and helper
//! functions used by the server and supporting tools. Only one database
//! backend (either `sqlite` or `postgres`) should be enabled at a time.
#[cfg(all(feature = "sqlite", feature = "postgres"))]
compile_error!("Choose either sqlite or postgres, not both");

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("Either the 'sqlite' or 'postgres' feature must be enabled");

#[cfg(feature = "postgres")]
pub use diesel::pg::Pg as DbBackend;
#[cfg(feature = "sqlite")]
pub use diesel::sqlite::Sqlite as DbBackend;

pub mod commands;
pub mod db;
pub mod field_id;
pub mod handler;
pub mod header_util;
pub mod login;
pub mod models;
pub(crate) mod news_path;
pub mod protocol;
pub mod schema;
pub mod transaction;
pub mod transaction_type;
pub mod users;
