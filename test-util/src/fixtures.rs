//! Database fixtures used by integration tests.
//!
//! Centralises repeated setup flows (users, files, news content) so tests can
//! compose databases with minimal boilerplate.

use std::{collections::HashMap, io};

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncConnection, RunQueryDsl};
use futures_util::future::BoxFuture;
use mxd::{
    db::{
        DbConnection,
        add_file_acl,
        apply_migrations,
        create_bundle,
        create_category,
        create_file,
        create_user,
    },
    models::{NewArticle, NewBundle, NewCategory, NewFileAcl, NewFileEntry, NewUser},
    schema::{files::dsl as files_dsl, users::dsl as users_dsl},
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
    pub const fn as_str(&self) -> &str { self.0.as_str() }
}

impl From<&str> for DatabaseUrl {
    fn from(value: &str) -> Self { Self::new(value) }
}

/// Resolve a file name to its ID from the lookup map.
fn resolve_file_id(file_ids: &HashMap<String, i32>, name: &str) -> Result<i32, AnyError> {
    file_ids
        .get(name)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("missing file id for {name}"))
}

/// Ensure the test user 'alice' exists in the database.
///
/// This helper is idempotent; it checks for the user first and creates only if
/// not present.
async fn ensure_test_user(conn: &mut DbConnection) -> Result<(), AnyError> {
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
pub fn with_db<F>(db: DatabaseUrl, f: F) -> Result<(), AnyError>
where
    F: for<'c> FnOnce(&'c mut DbConnection) -> BoxFuture<'c, Result<(), AnyError>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut conn = DbConnection::establish(db.as_str()).await?;
        apply_migrations(&mut conn, db.as_str()).await?;
        f(&mut conn).await
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
            let user_id: i32 = users_dsl::users
                .filter(users_dsl::username.eq("alice"))
                .select(users_dsl::id)
                .first(conn)
                .await?;
            let files = [
                NewFileEntry {
                    name: "fileA.txt",
                    object_key: "1",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileB.txt",
                    object_key: "2",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileC.txt",
                    object_key: "3",
                    size: 1,
                },
            ];
            for file in &files {
                create_file(conn, file).await?;
            }
            let file_rows = files_dsl::files
                .select((files_dsl::name, files_dsl::id))
                .load::<(String, i32)>(conn)
                .await?;
            let file_ids: HashMap<_, _> = file_rows.into_iter().collect();
            for name in ["fileA.txt", "fileC.txt"] {
                let file_id = resolve_file_id(&file_ids, name)?;
                add_file_acl(conn, &NewFileAcl { file_id, user_id }).await?;
            }
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

async fn insert_root_bundle(conn: &mut DbConnection) -> Result<i32, AnyError> {
    let id = create_bundle(
        conn,
        &NewBundle {
            parent_bundle_id: None,
            name: "Bundle",
        },
    )
    .await?;

    Ok(id)
}

async fn insert_article(
    conn: &mut DbConnection,
    article: &NewArticle<'_>,
) -> Result<i32, AnyError> {
    use mxd::schema::news_articles::dsl as a;

    #[cfg(feature = "postgres")]
    let inserted_id: i32 = diesel::insert_into(a::news_articles)
        .values(article)
        .returning(a::id)
        .get_result(conn)
        .await?;

    #[cfg(not(feature = "postgres"))]
    let inserted_id: i32 = {
        use diesel::sql_types::Integer;
        diesel::insert_into(a::news_articles)
            .values(article)
            .execute(conn)
            .await?;
        diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()"))
            .get_result(conn)
            .await?
    };

    Ok(inserted_id)
}
