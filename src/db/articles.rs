//! Article helpers layered atop bundle/category path resolution.

#![allow(
    clippy::shadow_reuse,
    reason = "intentional shadowing in transaction closures"
)]

use chrono::Utc;
use diesel::prelude::*;
use diesel_async::{AsyncConnection, RunQueryDsl};

#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
use super::insert::fetch_last_insert_rowid;
use super::{categories::category_id_from_path, connection::DbConnection, paths::PathLookupError};

/// Retrieve a single article by path and identifier.
///
/// # Errors
/// Returns an error if the path is invalid or the query fails.
#[must_use = "handle the result"]
pub async fn get_article(
    conn: &mut DbConnection,
    path: &str,
    article_id: i32,
) -> Result<Option<crate::models::Article>, PathLookupError> {
    use crate::schema::news_articles::dsl as a;
    let cat_id = category_id_from_path(conn, path).await?;
    let found = a::news_articles
        .filter(a::category_id.eq(cat_id))
        .filter(a::id.eq(article_id))
        .first::<crate::models::Article>(conn)
        .await
        .optional()?;
    Ok(found)
}

/// List the titles of all root-level articles within a category.
///
/// # Errors
/// Returns an error if the path is invalid or the query fails.
#[must_use = "handle the result"]
pub async fn list_article_titles(
    conn: &mut DbConnection,
    path: &str,
) -> Result<Vec<String>, PathLookupError> {
    use crate::schema::news_articles::dsl as a;
    let cat_id = category_id_from_path(conn, path).await?;
    let titles = a::news_articles
        .filter(a::category_id.eq(cat_id))
        .filter(a::parent_article_id.is_null())
        .order(a::posted_at.asc())
        .select(a::title)
        .load::<String>(conn)
        .await
        .map_err(PathLookupError::Diesel)?;
    Ok(titles)
}

/// Parameters required to create a new root article.
pub struct CreateRootArticleParams<'a> {
    /// Article title.
    pub title: &'a str,
    /// Article flags.
    pub flags: i32,
    /// Data content type.
    pub data_flavor: &'a str,
    /// Article content.
    pub data: &'a str,
}

/// Create a new root article in the specified category path.
///
/// # Errors
/// Returns an error if the path is invalid or the insertion fails.
#[must_use = "handle the result"]
pub async fn create_root_article(
    conn: &mut DbConnection,
    path: &str,
    params: CreateRootArticleParams<'_>,
) -> Result<i32, PathLookupError> {
    conn.transaction::<_, PathLookupError, _>(|conn| {
        Box::pin(async move {
            let cat_id = category_id_from_path(conn, path).await?;
            let last = get_last_root_article_id(conn, cat_id).await?;
            let inserted = insert_new_article(conn, cat_id, last, &params).await?;
            if let Some(prev) = last {
                link_prev_to_new(conn, prev, inserted).await?;
            }
            Ok(inserted)
        })
    })
    .await
}

async fn get_last_root_article_id(
    conn: &mut DbConnection,
    cat_id: i32,
) -> Result<Option<i32>, PathLookupError> {
    use crate::schema::news_articles::dsl as a;
    a::news_articles
        .filter(a::category_id.eq(cat_id))
        .filter(a::parent_article_id.is_null())
        .order(a::id.desc())
        .select(a::id)
        .first::<i32>(conn)
        .await
        .optional()
        .map_err(PathLookupError::Diesel)
}

async fn insert_new_article(
    conn: &mut DbConnection,
    cat_id: i32,
    prev: Option<i32>,
    params: &CreateRootArticleParams<'_>,
) -> Result<i32, PathLookupError> {
    use crate::schema::news_articles::dsl as a;
    let now = Utc::now().naive_utc();
    let article = crate::models::NewArticle {
        category_id: cat_id,
        parent_article_id: None,
        prev_article_id: prev,
        next_article_id: None,
        first_child_article_id: None,
        title: params.title,
        poster: None,
        posted_at: now,
        flags: params.flags,
        data_flavor: Some(params.data_flavor),
        data: Some(params.data),
    };

    #[cfg(any(feature = "postgres", feature = "returning_clauses_for_sqlite_3_35"))]
    {
        diesel::insert_into(a::news_articles)
            .values(&article)
            .returning(a::id)
            .get_result(conn)
            .await
            .map_err(PathLookupError::Diesel)
    }

    #[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
    {
        diesel::insert_into(a::news_articles)
            .values(&article)
            .execute(conn)
            .await
            .map_err(PathLookupError::Diesel)?;
        fetch_last_insert_rowid(conn)
            .await
            .map_err(PathLookupError::Diesel)
    }
}

async fn link_prev_to_new(
    conn: &mut DbConnection,
    prev: i32,
    new_id: i32,
) -> Result<(), PathLookupError> {
    use crate::schema::news_articles::dsl as a;
    diesel::update(a::news_articles.filter(a::id.eq(prev)))
        .set(a::next_article_id.eq(new_id))
        .execute(conn)
        .await
        .map_err(PathLookupError::Diesel)?;
    Ok(())
}
