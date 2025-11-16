//! Article helpers layered atop bundle/category path resolution.

use chrono::Utc;
use diesel::prelude::*;
use diesel_async::{AsyncConnection, RunQueryDsl};

use super::{
    bundles::PathLookupError,
    categories::category_id_from_path,
    connection::DbConnection,
};

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

/// Create a new root article in the specified category path.
///
/// # Errors
/// Returns an error if the path is invalid or the insertion fails.
#[must_use = "handle the result"]
pub async fn create_root_article(
    conn: &mut DbConnection,
    path: &str,
    title: &str,
    flags: i32,
    data_flavor: &str,
    data: &str,
) -> Result<i32, PathLookupError> {
    use crate::schema::news_articles::dsl as a;

    conn.transaction::<_, PathLookupError, _>(|conn| {
        Box::pin(async move {
            let cat_id = category_id_from_path(conn, path).await?;

            let last_id: Option<i32> = a::news_articles
                .filter(a::category_id.eq(cat_id))
                .filter(a::parent_article_id.is_null())
                .order(a::id.desc())
                .select(a::id)
                .first::<i32>(conn)
                .await
                .optional()?;

            let now = Utc::now().naive_utc();
            let article = crate::models::NewArticle {
                category_id: cat_id,
                parent_article_id: None,
                prev_article_id: last_id,
                next_article_id: None,
                first_child_article_id: None,
                title,
                poster: None,
                posted_at: now,
                flags,
                data_flavor: Some(data_flavor),
                data: Some(data),
            };

            #[cfg(any(feature = "postgres", feature = "returning_clauses_for_sqlite_3_35"))]
            let inserted_id: i32 = diesel::insert_into(a::news_articles)
                .values(&article)
                .returning(a::id)
                .get_result(conn)
                .await?;

            #[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
            let inserted_id: i32 = {
                use diesel::sql_types::Integer;

                diesel::insert_into(a::news_articles)
                    .values(&article)
                    .execute(conn)
                    .await?;

                diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()"))
                    .get_result(conn)
                    .await?
            };

            if let Some(prev) = last_id {
                diesel::update(a::news_articles.filter(a::id.eq(prev)))
                    .set(a::next_article_id.eq(inserted_id))
                    .execute(conn)
                    .await?;
            }
            Ok(inserted_id)
        })
    })
    .await
}
