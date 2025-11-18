//! Bundle helpers and shared path resolution types.

use cfg_if::cfg_if;
use diesel::{
    OptionalExtension,
    QueryableByName,
    prelude::*,
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
use crate::{
    models::{Bundle, Category},
    news_path::{BUNDLE_BODY_SQL, BUNDLE_STEP_SQL, build_path_cte_with_conn},
};

async fn bundle_id_from_path(
    conn: &mut DbConnection,
    path: &str,
) -> Result<Option<i32>, PathLookupError> {
    #[derive(QueryableByName)]
    struct BunId {
        #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
        id: Option<i32>,
    }

    let Some((json, len)) = parse_path_segments(path, true)? else {
        return Ok(None);
    };

    let step = sql_query(BUNDLE_STEP_SQL).bind::<Text, _>(json.clone());
    let len_i32: i32 = i32::try_from(len).map_err(|_| PathLookupError::InvalidPath)?;
    let body = sql_query(BUNDLE_BODY_SQL).bind::<Integer, _>(len_i32);

    let query = build_path_cte_with_conn(conn, step, body);
    let res: Option<BunId> = query.get_result(conn).await.optional()?;
    normalize_lookup_result(res.and_then(|b| b.id), true)
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
    let mut names: Vec<String> = apply_parent_filter(
        b::news_bundles.into_boxed(),
        bundle_id,
        |q, id| q.filter(b::parent_bundle_id.eq(id)),
        |q| q.filter(b::parent_bundle_id.is_null()),
    )
    .order(b::name.asc())
    .load::<Bundle>(conn)
    .await?
    .into_iter()
    .map(|b| b.name)
    .collect();
    let mut cats: Vec<String> = apply_parent_filter(
        c::news_categories.into_boxed(),
        bundle_id,
        |q, id| q.filter(c::bundle_id.eq(id)),
        |q| q.filter(c::bundle_id.is_null()),
    )
    .order(c::name.asc())
    .load::<Category>(conn)
    .await?
    .into_iter()
    .map(|c| c.name)
    .collect();
    names.append(&mut cats);
    Ok(names)
}

fn apply_parent_filter<Q, FSome, FNone>(
    query: Q,
    parent: Option<i32>,
    when_some: FSome,
    when_none: FNone,
) -> Q
where
    FSome: FnOnce(Q, i32) -> Q,
    FNone: FnOnce(Q) -> Q,
{
    if let Some(id) = parent {
        when_some(query, id)
    } else {
        when_none(query)
    }
}

cfg_if! {
    if #[cfg(any(feature = "postgres", feature = "returning_clauses_for_sqlite_3_35"))] {
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
            diesel::insert_into(news_bundles)
                .values(bun)
                .execute(conn)
                .await?;
            fetch_last_insert_rowid(conn).await
        }
    } else {
        compile_error!("Either 'sqlite' or 'postgres' feature must be enabled");
    }
}
