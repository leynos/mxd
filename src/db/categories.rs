//! Category helpers including path resolution.

use cfg_if::cfg_if;
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

cfg_if! {
    if #[cfg(any(feature = "postgres", feature = "returning_clauses_for_sqlite_3_35"))] {
        /// Insert a new news category.
        ///
        /// # Errors
        /// Returns any error produced by the database.
        #[must_use = "handle the result"]
        pub async fn create_category(
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
    } else if #[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))] {
        /// Insert a new news category.
        ///
        /// # Errors
        /// Returns any error produced by the database.
        #[must_use = "handle the result"]
        pub async fn create_category(
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
    } else {
        compile_error!("Either 'sqlite' or 'postgres' feature must be enabled");
    }
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
    let step = sql_query(CATEGORY_STEP_SQL).bind::<Text, _>(json.clone());
    let len_minus_one: i32 = i32::try_from(len - 1).map_err(|_| PathLookupError::InvalidPath)?;
    let body = sql_query(CATEGORY_BODY_SQL)
        .bind::<Text, _>(json)
        .bind::<Integer, _>(len_minus_one)
        .bind::<Integer, _>(len_minus_one);

    let query = build_path_cte_with_conn(conn, step, body);
    let res: Option<CatId> = query.get_result(conn).await.optional()?;
    normalize_lookup_result(res.map(|c| c.id), true)?.ok_or(PathLookupError::InvalidPath)
}
