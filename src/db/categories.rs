//! Category helpers including path resolution.

use cfg_if::cfg_if;
use diesel::{
    prelude::*,
    result::QueryResult,
    sql_query,
    sql_types::{Integer, Text},
};
use diesel_async::RunQueryDsl;

use super::{bundles::PathLookupError, connection::DbConnection};
use crate::news_path::{
    CATEGORY_BODY_SQL,
    CATEGORY_STEP_SQL,
    build_path_cte_with_conn,
    prepare_path,
};

cfg_if! {
    if #[cfg(feature = "postgres")] {
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
    } else if #[cfg(all(feature = "sqlite", feature = "returning_clauses_for_sqlite_3_35"))] {
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
            use diesel::sql_types::Integer;
            diesel::insert_into(news_categories)
                .values(cat)
                .execute(conn)
                .await?;
            diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()"))
                .get_result(conn)
                .await
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

    let Some((json, len)) = prepare_path(path)? else {
        return Err(PathLookupError::InvalidPath);
    };

    // Step advances the tree by joining the next path segment from json_each
    // against the bundles table.
    let step = sql_query(CATEGORY_STEP_SQL).bind::<Text, _>(json.clone());

    // Body selects the category matching the final path segment and bundle.
    let len_minus_one: i32 = i32::try_from(len - 1).map_err(|_| PathLookupError::InvalidPath)?;
    let body = sql_query(CATEGORY_BODY_SQL)
        .bind::<Text, _>(json)
        .bind::<Integer, _>(len_minus_one)
        .bind::<Integer, _>(len_minus_one);

    let query = build_path_cte_with_conn(conn, step, body);

    let res: Option<CatId> = query.get_result(conn).await.optional()?;
    res.map(|c| c.id).ok_or(PathLookupError::InvalidPath)
}
