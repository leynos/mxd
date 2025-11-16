//! Bundle helpers and shared path resolution types.

use cfg_if::cfg_if;
use diesel::{
    prelude::*,
    result::QueryResult,
    sql_query,
    sql_types::{Integer, Text},
};
use diesel_async::RunQueryDsl;
use thiserror::Error;

use super::connection::DbConnection;
use crate::{
    models::{Bundle, Category},
    news_path::{BUNDLE_BODY_SQL, BUNDLE_STEP_SQL, build_path_cte_with_conn, prepare_path},
};

#[derive(Debug, Error)]
pub enum PathLookupError {
    #[error("invalid news path")]
    InvalidPath,
    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

async fn bundle_id_from_path(
    conn: &mut DbConnection,
    path: &str,
) -> Result<Option<i32>, PathLookupError> {
    #[derive(QueryableByName)]
    struct BunId {
        #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
        id: Option<i32>,
    }

    let Some((json, len)) = prepare_path(path)? else {
        return Ok(None);
    };

    let step = sql_query(BUNDLE_STEP_SQL).bind::<Text, _>(json.clone());
    let len_i32: i32 = i32::try_from(len).map_err(|_| PathLookupError::InvalidPath)?;
    let body = sql_query(BUNDLE_BODY_SQL).bind::<Integer, _>(len_i32);

    let query = build_path_cte_with_conn(conn, step, body);

    let res: Option<BunId> = query.get_result(conn).await.optional()?;
    match res.and_then(|b| b.id) {
        Some(id) => Ok(Some(id)),
        None => Err(PathLookupError::InvalidPath),
    }
}

/// List bundle and category names located at the given path.
///
/// # Errors
/// Returns an error if the path is invalid or the query fails.
#[must_use = "handle the result"]
pub async fn list_names_at_path(
    conn: &mut DbConnection,
    path: Option<&str>,
) -> Result<Vec<String>, PathLookupError> {
    use crate::schema::{news_bundles::dsl as b, news_categories::dsl as c};
    let bundle_id = if let Some(p) = path {
        bundle_id_from_path(conn, p).await?
    } else {
        None
    };
    let mut bundle_query = b::news_bundles.into_boxed();
    if let Some(id) = bundle_id {
        bundle_query = bundle_query.filter(b::parent_bundle_id.eq(id));
    } else {
        bundle_query = bundle_query.filter(b::parent_bundle_id.is_null());
    }
    let mut names: Vec<String> = bundle_query
        .order(b::name.asc())
        .load::<Bundle>(conn)
        .await?
        .into_iter()
        .map(|b| b.name)
        .collect();
    let mut cat_query = c::news_categories.into_boxed();
    if let Some(id) = bundle_id {
        cat_query = cat_query.filter(c::bundle_id.eq(id));
    } else {
        cat_query = cat_query.filter(c::bundle_id.is_null());
    }
    let mut cats: Vec<String> = cat_query
        .order(c::name.asc())
        .load::<Category>(conn)
        .await?
        .into_iter()
        .map(|c| c.name)
        .collect();
    names.append(&mut cats);
    Ok(names)
}

cfg_if! {
    if #[cfg(feature = "postgres")] {
        /// Insert a new news bundle.
        ///
        /// # Errors
        /// Returns any error produced by the database.
        #[must_use = "handle the result"]
        pub async fn create_bundle(
            conn: &mut DbConnection,
            bun: &crate::models::NewBundle<'_>,
        ) -> QueryResult<i32> {
            use crate::schema::news_bundles::dsl::{id, news_bundles};
            diesel::insert_into(news_bundles)
                .values(bun)
                .returning(id)
                .get_result(conn)
                .await
        }
    } else if #[cfg(all(feature = "sqlite", feature = "returning_clauses_for_sqlite_3_35"))] {
        /// Insert a new news bundle.
        ///
        /// # Errors
        /// Returns any error produced by the database.
        #[must_use = "handle the result"]
        pub async fn create_bundle(
            conn: &mut DbConnection,
            bun: &crate::models::NewBundle<'_>,
        ) -> QueryResult<i32> {
            use crate::schema::news_bundles::dsl::{id, news_bundles};
            diesel::insert_into(news_bundles)
                .values(bun)
                .returning(id)
                .get_result(conn)
                .await
        }
    } else if #[cfg(all(feature = "sqlite", not(feature = "returning_clauses_for_sqlite_3_35")))] {
        /// Insert a new news bundle.
        ///
        /// # Errors
        /// Returns any error produced by the database.
        #[must_use = "handle the result"]
        pub async fn create_bundle(
            conn: &mut DbConnection,
            bun: &crate::models::NewBundle<'_>,
        ) -> QueryResult<i32> {
            use crate::schema::news_bundles::dsl::news_bundles;
            use diesel::sql_types::Integer;
            diesel::insert_into(news_bundles)
                .values(bun)
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
