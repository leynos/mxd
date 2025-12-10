//! Category helpers including path resolution.

use diesel::{
    OptionalExtension,
    QueryableByName,
    result::QueryResult,
    sql_query,
    sql_types::{Integer, Text},
};
use diesel_async::RunQueryDsl;

#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
use super::insert::fetch_last_insert_rowid;
use super::{
    connection::DbConnection,
    paths::{PathLookupError, normalize_lookup_result, parse_path_segments},
};
use crate::news_path::{CATEGORY_BODY_SQL, CATEGORY_STEP_SQL, build_path_cte_with_conn};

#[cfg(any(feature = "postgres", feature = "returning_clauses_for_sqlite_3_35"))]
async fn create_category_inner(
    conn: &mut DbConnection,
    cat: &crate::models::NewCategory<'_>,
) -> QueryResult<i32> {
    use crate::schema::news_categories::dsl::{id, news_categories};
    diesel::insert_into(news_categories)
        .values(cat)
        .returning(id)
        .get_result(conn)
        .await
}

#[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))]
async fn create_category_inner(
    conn: &mut DbConnection,
    cat: &crate::models::NewCategory<'_>,
) -> QueryResult<i32> {
    use crate::schema::news_categories::dsl::news_categories;
    diesel::insert_into(news_categories)
        .values(cat)
        .execute(conn)
        .await?;
    fetch_last_insert_rowid(conn).await
}

/// Insert a new news category.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
pub async fn create_category(
    conn: &mut DbConnection,
    cat: &crate::models::NewCategory<'_>,
) -> QueryResult<i32> {
    create_category_inner(conn, cat).await
}

pub(super) async fn category_id_from_path(
    conn: &mut DbConnection,
    path: &str,
) -> Result<i32, PathLookupError> {
    #[derive(QueryableByName)]
    struct CatId {
        #[diesel(sql_type = Integer)]
        id: i32,
    }

    let Some((json, len)) = parse_path_segments(path, false)? else {
        return Err(PathLookupError::InvalidPath);
    };

    if len == 0 {
        return Err(PathLookupError::InvalidPath);
    }
    let step = sql_query(CATEGORY_STEP_SQL).bind::<Text, _>(json);
    let len_minus_one: i32 = i32::try_from(len - 1).map_err(|_| PathLookupError::InvalidPath)?;
    let trimmed = path.trim_matches('/');
    let final_segment = trimmed
        .rsplit('/')
        .next()
        .ok_or(PathLookupError::InvalidPath)?
        .to_owned();
    let body = sql_query(CATEGORY_BODY_SQL)
        .bind::<Text, _>(final_segment)
        .bind::<Integer, _>(len_minus_one);

    let query = build_path_cte_with_conn(conn, step, body);
    let res: Option<CatId> = query.get_result(conn).await.optional()?;
    let maybe_id = normalize_lookup_result(res.map(|c| c.id), true)?;
    maybe_id.map_or_else(|| Err(PathLookupError::InvalidPath), Ok)
}
