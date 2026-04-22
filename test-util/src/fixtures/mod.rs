//! Database fixtures used by integration tests.
//!
//! Centralizes repeated setup flows (users, files, news content) so tests can
//! compose databases with minimal boilerplate.

mod helpers;

use std::{collections::HashMap, io};

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncConnection, RunQueryDsl};
use futures_util::future::BoxFuture;
use helpers::{insert_article, insert_root_bundle};
use mxd::{
    db::{
        DbConnection,
        add_user_to_group,
        apply_migrations,
        create_bundle,
        create_category,
        create_file_node,
        create_group,
        create_user,
        download_file_permission,
        grant_resource_permission,
        seed_permission,
    },
    models::{
        FileNodeKind,
        NewArticle,
        NewBundle,
        NewCategory,
        NewFileNode,
        NewGroup,
        NewResourcePermission,
        NewUser,
        NewUserGroup,
    },
    schema::users::dsl as users_dsl,
    users::hash_password,
};

use crate::AnyError;

/// Database URL wrapper to make fixture APIs more explicit.
#[derive(Clone, Debug)]
pub struct DatabaseUrl(String);

impl DatabaseUrl {
    /// Create a new database URL wrapper from a string.
    pub fn new(url: impl Into<String>) -> Self { Self(url.into()) }

    /// Borrow the wrapped URL as a string slice.
    #[must_use]
    #[expect(
        clippy::missing_const_for_fn,
        reason = "String::as_str is not const-stable on Rust 1.85"
    )]
    pub fn as_str(&self) -> &str { self.0.as_str() }
}

impl From<&str> for DatabaseUrl {
    fn from(value: &str) -> Self { Self::new(value) }
}

impl From<String> for DatabaseUrl {
    fn from(value: String) -> Self { Self::new(value) }
}

impl From<&crate::server::DbUrl> for DatabaseUrl {
    fn from(value: &crate::server::DbUrl) -> Self { Self::new(value.as_ref()) }
}

impl AsRef<str> for DatabaseUrl {
    fn as_ref(&self) -> &str { self.as_str() }
}

/// Resolve a file name to its file-node ID from the lookup map.
fn resolve_file_node_id(file_node_ids: &HashMap<String, i32>, name: &str) -> Result<i32, AnyError> {
    file_node_ids
        .get(name)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("missing file-node id for {name}"))
}

async fn fetch_test_user_id(conn: &mut DbConnection) -> Result<i32, AnyError> {
    users_dsl::users
        .filter(users_dsl::username.eq("alice"))
        .select(users_dsl::id)
        .first(conn)
        .await
        .map_err(Into::into)
}

async fn seed_download_file_permission(conn: &mut DbConnection) -> Result<i32, AnyError> {
    seed_permission(conn, &download_file_permission())
        .await
        .map_err(Into::into)
}

async fn ensure_everyone_group_membership(
    conn: &mut DbConnection,
    user_id: i32,
) -> Result<(), AnyError> {
    let everyone_group_id = create_group(conn, &NewGroup { name: "everyone" }).await?;
    let _group_added = add_user_to_group(
        conn,
        &NewUserGroup {
            user_id,
            group_id: everyone_group_id,
        },
    )
    .await?;
    Ok(())
}

async fn seed_root_file_nodes(
    conn: &mut DbConnection,
    creator_id: i32,
) -> Result<HashMap<String, i32>, AnyError> {
    let file_nodes = [
        NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "fileA.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("1"),
            size: Some(1),
            comment: None,
            is_dropbox: false,
            creator_id,
        },
        NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "fileB.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("2"),
            size: Some(1),
            comment: None,
            is_dropbox: false,
            creator_id,
        },
        NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "fileC.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("3"),
            size: Some(1),
            comment: None,
            is_dropbox: false,
            creator_id,
        },
    ];
    let mut file_node_ids = HashMap::with_capacity(file_nodes.len());
    for file_node in &file_nodes {
        let node_id = create_file_node(conn, file_node).await?;
        file_node_ids.insert(file_node.name.to_owned(), node_id);
    }
    Ok(file_node_ids)
}

async fn grant_fixture_download_visibility(
    conn: &mut DbConnection,
    user_id: i32,
    permission_id: i32,
    file_node_ids: &HashMap<String, i32>,
) -> Result<(), AnyError> {
    for name in ["fileA.txt", "fileC.txt"] {
        let resource_id = resolve_file_node_id(file_node_ids, name)?;
        grant_resource_permission(
            conn,
            &NewResourcePermission {
                resource_type: "file_node",
                resource_id,
                principal_type: "user",
                principal_id: user_id,
                permission_id,
            },
        )
        .await?;
    }
    Ok(())
}

/// Ensure the test user 'alice' exists in the database.
///
/// This helper is idempotent; it checks for the user first and creates only if
/// not present.
///
/// # Errors
///
/// Returns an error if the user lookup, password hashing, or creation fails.
pub async fn ensure_test_user(conn: &mut DbConnection) -> Result<(), AnyError> {
    let existing = users_dsl::users
        .filter(users_dsl::username.eq("alice"))
        .select(users_dsl::id)
        .first::<i32>(conn)
        .await
        .optional()?;
    if existing.is_none() {
        let argon2 = argon2::Argon2::default();
        let hashed = hash_password(&argon2, "secret")?;
        let new_user = NewUser {
            username: "alice",
            password: &hashed,
        };
        create_user(conn, &new_user).await?;
    }
    Ok(())
}

/// Execute a database operation within a connection.
///
/// Establishes a connection, runs migrations, and executes the provided closure.
///
/// # Errors
///
/// Returns an error if the connection cannot be established, migrations fail,
/// or the closure returns an error.
#[expect(
    clippy::needless_pass_by_value,
    reason = "DatabaseUrl is an owned API boundary for fixtures"
)]
pub fn with_db<F, R>(db: DatabaseUrl, f: F) -> Result<R, AnyError>
where
    F: for<'c> FnOnce(&'c mut DbConnection) -> BoxFuture<'c, Result<R, AnyError>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut conn = DbConnection::establish(db.as_str()).await?;
        apply_migrations(&mut conn, db.as_str()).await?;
        f(&mut conn).await
    })
}

/// Create a test database that contains only the default login user.
///
/// # Errors
///
/// Returns an error if database setup fails.
pub fn setup_login_db(db: DatabaseUrl) -> Result<(), AnyError> {
    with_db(db, |conn| {
        Box::pin(async move {
            ensure_test_user(conn).await?;
            Ok(())
        })
    })
}

/// Create a test database with users and files for ACL testing.
///
/// # Errors
///
/// Returns an error if database setup fails.
pub fn setup_files_db(db: DatabaseUrl) -> Result<(), AnyError> {
    with_db(db, |conn| {
        Box::pin(async move {
            ensure_test_user(conn).await?;
            let user_id = fetch_test_user_id(conn).await?;
            let permission_id = seed_download_file_permission(conn).await?;
            ensure_everyone_group_membership(conn, user_id).await?;
            let file_node_ids = seed_root_file_nodes(conn, user_id).await?;
            grant_fixture_download_visibility(conn, user_id, permission_id, &file_node_ids).await?;
            Ok(())
        })
    })
}

/// Create a test database with news categories and articles.
///
/// # Errors
///
/// Returns an error if database setup fails.
pub fn setup_news_db(db: DatabaseUrl) -> Result<(), AnyError> {
    with_db(db, |conn| {
        Box::pin(async move {
            // Ensure test user exists for authentication
            ensure_test_user(conn).await?;

            let category_id = create_category(
                conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;

            let posted = DateTime::<Utc>::from_timestamp(1000, 0)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "news fixture timestamp out of range",
                    )
                })?
                .naive_utc();
            let first_article_id = insert_article(
                conn,
                &NewArticle {
                    category_id,
                    parent_article_id: None,
                    prev_article_id: None,
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "First",
                    poster: None,
                    posted_at: posted,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("a"),
                },
            )
            .await?;

            let posted2 = DateTime::<Utc>::from_timestamp(2000, 0)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "news fixture timestamp out of range",
                    )
                })?
                .naive_utc();
            insert_article(
                conn,
                &NewArticle {
                    category_id,
                    parent_article_id: None,
                    prev_article_id: Some(first_article_id),
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "Second",
                    poster: None,
                    posted_at: posted2,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("b"),
                },
            )
            .await?;
            Ok(())
        })
    })
}

/// Create a test database with one news category and a single article.
///
/// Returns the inserted article ID for tests that need to fetch it.
///
/// # Errors
///
/// Returns an error if database setup fails.
pub fn setup_news_with_article(db: DatabaseUrl) -> Result<i32, AnyError> {
    with_db(db, |conn| {
        Box::pin(async move {
            ensure_test_user(conn).await?;

            let category_id = create_category(
                conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;

            let posted = DateTime::<Utc>::from_timestamp(1000, 0)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "news fixture timestamp out of range",
                    )
                })?
                .naive_utc();
            let inserted_id = insert_article(
                conn,
                &NewArticle {
                    category_id,
                    parent_article_id: None,
                    prev_article_id: None,
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "First",
                    poster: Some("alice"),
                    posted_at: posted,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("hello"),
                },
            )
            .await?;
            Ok(inserted_id)
        })
    })
}

/// Create a test database with root-level news categories.
///
/// # Errors
///
/// Returns an error if database setup fails.
pub fn setup_news_categories_root_db(db: DatabaseUrl) -> Result<(), AnyError> {
    setup_news_categories_with_structure(db, |conn, _| {
        Box::pin(async move {
            create_category(
                conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            create_category(
                conn,
                &NewCategory {
                    name: "Updates",
                    bundle_id: None,
                },
            )
            .await?;
            Ok(())
        })
    })
}

/// Create a test database with nested news categories.
///
/// # Errors
///
/// Returns an error if database setup fails.
pub fn setup_news_categories_nested_db(db: DatabaseUrl) -> Result<(), AnyError> {
    setup_news_categories_with_structure(db, |conn, root_id| {
        Box::pin(async move {
            let sub_id = create_bundle(
                conn,
                &NewBundle {
                    parent_bundle_id: Some(root_id),
                    name: "Sub",
                },
            )
            .await?;

            create_category(
                conn,
                &NewCategory {
                    name: "Inside",
                    bundle_id: Some(sub_id),
                },
            )
            .await?;
            Ok(())
        })
    })
}

/// Create a test database with custom news category structure.
///
/// The provided closure receives the root bundle ID to build upon.
///
/// # Errors
///
/// Returns an error if database setup fails.
pub fn setup_news_categories_with_structure<F>(db: DatabaseUrl, build: F) -> Result<(), AnyError>
where
    F: Send
        + 'static
        + for<'c> FnOnce(&'c mut DbConnection, i32) -> BoxFuture<'c, Result<(), AnyError>>,
{
    with_db(db, |conn| {
        Box::pin(async move {
            // Ensure test user exists for authentication
            ensure_test_user(conn).await?;

            let root_id = insert_root_bundle(conn).await?;
            build(conn, root_id).await
        })
    })
}
