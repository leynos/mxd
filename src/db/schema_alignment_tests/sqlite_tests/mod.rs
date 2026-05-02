//! `SQLite` schema-alignment regression tests for roadmap item 4.1.1.
//!
//! Scope:
//! - Validates aligned schema on fresh migration and on upgrade from legacy rebuilds.
//! - Asserts bundle schema columns, category partial uniqueness at root, GUID non-emptiness and
//!   uniqueness, threading referential integrity, and article-index presence.
//!
//! Helpers:
//! - Uses in-memory connections, `sqlite_names` catalogue readers, and the parent module's backfill
//!   seed/assertion helpers.

mod common;
mod guid_and_counters;
mod schema_catalogue;
mod threading;
mod upgrade_flow;

pub(super) use self::schema_catalogue::assert_sqlite_article_indices;
