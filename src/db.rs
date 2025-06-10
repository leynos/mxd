use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::sync_connection_wrapper::SyncConnectionWrapper;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub type DbConnection = SyncConnectionWrapper<SqliteConnection>;
pub type DbPool = Pool<DbConnection>;

/// Create a pooled connection to the SQLite database.
///
/// # Panics
/// Panics if the connection pool cannot be created.
pub async fn establish_pool(database_url: &str) -> DbPool {
    let config = AsyncDieselConnectionManager::<DbConnection>::new(database_url);
    Pool::builder()
        .build(config)
        .await
        .expect("Failed to create pool")
}

pub async fn run_migrations(conn: &mut DbConnection) -> QueryResult<()> {
    use diesel::result::Error as DieselError;
    conn.spawn_blocking(|c| {
        c.run_pending_migrations(MIGRATIONS)
            .map(|_| ())
            .map_err(|e| {
                DieselError::QueryBuilderError(Box::new(std::io::Error::other(e.to_string())))
            })
    })
    .await?;
    Ok(())
}

pub async fn get_user_by_name(
    conn: &mut DbConnection,
    name: &str,
) -> QueryResult<Option<crate::models::User>> {
    use crate::schema::users::dsl::*;
    users
        .filter(username.eq(name))
        .first::<crate::models::User>(conn)
        .await
        .optional()
}

pub async fn create_user(
    conn: &mut DbConnection,
    user: &crate::models::NewUser<'_>,
) -> QueryResult<usize> {
    use crate::schema::users::dsl::*;
    diesel::insert_into(users).values(user).execute(conn).await
}

use crate::news_path::{
    BUNDLE_BODY_SQL, BUNDLE_STEP_SQL, CATEGORY_BODY_SQL, CATEGORY_STEP_SQL, CTE_SEED_SQL,
    prepare_path,
};
use thiserror::Error;

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
    use diesel::sql_query;
    use diesel::sql_types::{Integer, Text};
    use diesel_cte_ext::with_recursive;

    #[derive(QueryableByName)]
    struct BunId {
        #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
        id: Option<i32>,
    }

    let (json, len) = match prepare_path(path)? {
        Some(t) => t,
        None => return Ok(None),
    };

    let seed = sql_query(CTE_SEED_SQL);
    let step = sql_query(BUNDLE_STEP_SQL).bind::<Text, _>(json.clone());
    let len_i32: i32 = i32::try_from(len).map_err(|_| PathLookupError::InvalidPath)?;
    let body = sql_query(BUNDLE_BODY_SQL).bind::<Integer, _>(len_i32);

    let query =
        with_recursive::<diesel::sqlite::Sqlite, _, _, _>("tree", &["idx", "id"], seed, step, body);

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
pub async fn list_names_at_path(
    conn: &mut DbConnection,
    path: Option<&str>,
) -> Result<Vec<String>, PathLookupError> {
    let bundle_id = if let Some(p) = path {
        bundle_id_from_path(conn, p).await?
    } else {
        None
    };
    use crate::schema::news_bundles::dsl as b;
    let mut bundle_query = b::news_bundles.into_boxed();
    if let Some(id) = bundle_id {
        bundle_query = bundle_query.filter(b::parent_bundle_id.eq(id));
    } else {
        bundle_query = bundle_query.filter(b::parent_bundle_id.is_null());
    }
    let mut names: Vec<String> = bundle_query
        .order(b::name.asc())
        .load::<crate::models::Bundle>(conn)
        .await?
        .into_iter()
        .map(|b| b.name)
        .collect();

    use crate::schema::news_categories::dsl as c;
    let mut cat_query = c::news_categories.into_boxed();
    if let Some(id) = bundle_id {
        cat_query = cat_query.filter(c::bundle_id.eq(id));
    } else {
        cat_query = cat_query.filter(c::bundle_id.is_null());
    }
    let mut cats: Vec<String> = cat_query
        .order(c::name.asc())
        .load::<crate::models::Category>(conn)
        .await?
        .into_iter()
        .map(|c| c.name)
        .collect();
    names.append(&mut cats);
    Ok(names)
}

pub async fn create_category(
    conn: &mut DbConnection,
    cat: &crate::models::NewCategory<'_>,
) -> QueryResult<usize> {
    use crate::schema::news_categories::dsl::*;
    diesel::insert_into(news_categories)
        .values(cat)
        .execute(conn)
        .await
}

pub async fn create_bundle(
    conn: &mut DbConnection,
    bun: &crate::models::NewBundle<'_>,
) -> QueryResult<usize> {
    use crate::schema::news_bundles::dsl::*;
    diesel::insert_into(news_bundles)
        .values(bun)
        .execute(conn)
        .await
}

/// Retrieve a single article by path and identifier.
///
/// # Errors
/// Returns an error if the path is invalid or the query fails.
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

async fn category_id_from_path(
    conn: &mut DbConnection,
    path: &str,
) -> Result<i32, PathLookupError> {
    use diesel::sql_query;
    use diesel::sql_types::{Integer, Text};
    use diesel_cte_ext::with_recursive;

    #[derive(QueryableByName)]
    struct CatId {
        #[diesel(sql_type = Integer)]
        id: i32,
    }

    let Some((json, len)) = prepare_path(path)? else {
        return Err(PathLookupError::InvalidPath);
    };

    let seed = sql_query(CTE_SEED_SQL);

    // Step advances the tree by joining the next path segment from json_each
    // against the bundles table.
    let step = sql_query(CATEGORY_STEP_SQL).bind::<Text, _>(json.clone());

    // Body selects the category matching the final path segment and bundle.
    let len_minus_one: i32 = i32::try_from(len - 1).map_err(|_| PathLookupError::InvalidPath)?;
    let body = sql_query(CATEGORY_BODY_SQL)
        .bind::<Text, _>(json)
        .bind::<Integer, _>(len_minus_one)
        .bind::<Integer, _>(len_minus_one);

    let query =
        with_recursive::<diesel::sqlite::Sqlite, _, _, _>("tree", &["idx", "id"], seed, step, body);

    let res: Option<CatId> = query.get_result(conn).await.optional()?;
    res.map(|c| c.id).ok_or(PathLookupError::InvalidPath)
}

/// List the titles of all root-level articles within a category.
///
/// # Errors
/// Returns an error if the path is invalid or the query fails.
pub async fn list_article_titles(
    conn: &mut DbConnection,
    path: &str,
) -> Result<Vec<String>, PathLookupError> {
    let cat_id = category_id_from_path(conn, path).await?;
    use crate::schema::news_articles::dsl as a;
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
pub async fn create_root_article(
    conn: &mut DbConnection,
    path: &str,
    title: &str,
    flags: i32,
    data_flavor: &str,
    data: &str,
) -> Result<i32, PathLookupError> {
    use crate::schema::news_articles::dsl as a;
    use chrono::Utc;
    use diesel_async::AsyncConnection;

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

            let inserted_id: i32 = diesel::insert_into(a::news_articles)
                .values(&article)
                .returning(a::id)
                .get_result(conn)
                .await?;

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

/// Insert a new file metadata entry.
///
/// # Errors
/// Returns any error produced by the database.
pub async fn create_file(
    conn: &mut DbConnection,
    file: &crate::models::NewFileEntry<'_>,
) -> QueryResult<usize> {
    use crate::schema::files::dsl::files;
    diesel::insert_into(files).values(file).execute(conn).await
}

/// Add a file access control entry for a user.
///
/// # Errors
/// Returns any error produced by the database.
pub async fn add_file_acl(
    conn: &mut DbConnection,
    acl: &crate::models::NewFileAcl,
) -> QueryResult<bool> {
    use crate::schema::file_acl::dsl::file_acl;
    diesel::insert_into(file_acl)
        .values(acl)
        .on_conflict_do_nothing()
        .execute(conn)
        .await
        .map(|rows| rows > 0)
}

/// List all files accessible by the specified user.
///
/// # Errors
/// Returns any error produced by the database.
pub async fn list_files_for_user(
    conn: &mut DbConnection,
    uid: i32,
) -> QueryResult<Vec<crate::models::FileEntry>> {
    use crate::schema::file_acl::dsl as a;
    use crate::schema::files::dsl as f;
    f::files
        .inner_join(a::file_acl.on(a::file_id.eq(f::id)))
        .filter(a::user_id.eq(uid))
        .order(f::name.asc())
        .select((f::id, f::name, f::object_key, f::size))
        .load::<crate::models::FileEntry>(conn)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NewBundle, NewCategory, NewUser};
    use diesel_async::AsyncConnection;

    #[tokio::test]
    async fn test_create_and_get_user() {
        let mut conn = DbConnection::establish(":memory:").await.unwrap();
        run_migrations(&mut conn).await.unwrap();
        let new_user = NewUser {
            username: "alice",
            password: "hash",
        };
        create_user(&mut conn, &new_user).await.unwrap();
        let fetched = get_user_by_name(&mut conn, "alice").await.unwrap().unwrap();
        assert_eq!(fetched.username, "alice");
        assert_eq!(fetched.password, "hash");
    }

    // basic smoke test for migrations and insertion
    #[tokio::test]
    async fn test_create_bundle_and_category() {
        let mut conn = DbConnection::establish(":memory:").await.unwrap();
        run_migrations(&mut conn).await.unwrap();
        let bun = NewBundle {
            parent_bundle_id: None,
            name: "Bundle",
        };
        create_bundle(&mut conn, &bun).await.unwrap();
        let cat = NewCategory {
            name: "General",
            bundle_id: None,
        };
        create_category(&mut conn, &cat).await.unwrap();
        let _names = list_names_at_path(&mut conn, None).await.unwrap();
    }
}
