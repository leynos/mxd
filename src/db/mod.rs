//! Manage database connections and domain queries.
//!
//! This module tree exposes helpers for creating pooled Diesel connections,
//! running embedded migrations, auditing backend capabilities, and executing
//! application queries grouped by domain concerns.

mod articles;
mod audit;
mod bundles;
mod categories;
mod connection;
mod files;
mod insert;
mod migrations;
mod paths;
mod users;

#[cfg(test)]
mod tests;

#[cfg(feature = "postgres")]
pub use self::audit::audit_postgres_features;
#[cfg(feature = "sqlite")]
pub use self::audit::audit_sqlite_features;
pub use self::{
    articles::{CreateRootArticleParams, create_root_article, get_article, list_article_titles},
    bundles::{create_bundle, list_names_at_path},
    categories::create_category,
    connection::{Backend, DbConnection, DbPool, MIGRATIONS, establish_pool},
    files::{add_file_acl, create_file, list_files_for_user},
    migrations::{apply_migrations, run_migrations},
    paths::PathLookupError,
    users::{create_user, get_user_by_name},
};
