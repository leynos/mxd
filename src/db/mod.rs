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
mod file_path;
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
    files::{
        FileNodeLookupError,
        add_user_to_group,
        create_file_node,
        create_group,
        download_file_permission,
        get_file_node,
        grant_resource_permission,
        list_child_file_nodes,
        list_visible_root_file_nodes_for_user,
        resolve_alias_target,
        resolve_file_node_path,
        seed_permission,
    },
    migrations::{apply_migrations, run_migrations},
    paths::PathLookupError,
    users::{create_user, get_user_by_name},
};
