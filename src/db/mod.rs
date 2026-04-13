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
mod file_nodes;
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
    file_nodes::{
        FILE_NODE_RESOURCE_TYPE,
        FileNodeDescendant,
        USER_PRINCIPAL_TYPE,
        add_resource_permission,
        create_file_node,
        find_child_file_node,
        get_root_file_node,
        list_child_file_nodes,
        list_descendant_file_nodes,
        list_permitted_child_file_nodes_for_user,
    },
    files::{add_file_acl, create_file, list_files_for_user},
    migrations::{apply_migrations, run_migrations},
    paths::PathLookupError,
    users::{create_user, get_user_by_name},
};
