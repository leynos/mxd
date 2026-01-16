//! Private helpers for fixture setup.
//!
//! Provides database-agnostic functions to insert test data (bundles, articles)
//! used by integration and BDD tests. Conditional compilation ensures exactly
//! one database backend (`PostgreSQL` or `SQLite`) is active at compile time.

// Enforce that exactly one database backend is enabled at compile time.
#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("Either feature 'sqlite' or 'postgres' must be enabled");

// NOTE: The mutual exclusion of sqlite/postgres is NOT enforced at compile time
// because the lint feature configuration enables both features for static
// analysis coverage, so only enforce exclusivity outside lint runs.
#[cfg(all(feature = "sqlite", feature = "postgres", not(feature = "lint")))]
compile_error!("Choose either sqlite or postgres, not both");

use mxd::{
    db::{DbConnection, create_bundle},
    models::{NewArticle, NewBundle},
};

use crate::AnyError;

/// Creates and inserts a root bundle test fixture into the database.
///
/// # Parameters
/// - `conn`: Mutable reference to the database connection
///
/// # Returns
/// The ID of the newly created bundle on success, or an error.
pub(super) async fn insert_root_bundle(conn: &mut DbConnection) -> Result<i32, AnyError> {
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

/// Inserts a news article into the database.
///
/// Returns the ID of the newly inserted article.
///
/// # Parameters
/// - `conn`: Mutable reference to the database connection
/// - `article`: Article data to insert
///
/// # Backend-specific implementation
/// - `PostgreSQL` uses `returning(...).get_result()` to retrieve the ID
/// - `SQLite` uses `execute()` followed by `last_insert_rowid()`
///
/// # Errors
/// Returns an error if the database operation fails.
pub(super) async fn insert_article(
    conn: &mut DbConnection,
    article: &NewArticle<'_>,
) -> Result<i32, AnyError> {
    use diesel_async::RunQueryDsl;
    use mxd::schema::news_articles::dsl as a;

    #[cfg(feature = "postgres")]
    let inserted_id: i32 = diesel::insert_into(a::news_articles)
        .values(article)
        .returning(a::id)
        .get_result(conn)
        .await?;

    #[cfg(feature = "sqlite")]
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
