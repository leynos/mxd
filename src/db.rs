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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CategoryError {
    #[error("invalid news path")]
    InvalidPath,
    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),
}

async fn bundle_id_from_path(
    conn: &mut DbConnection,
    path: &str,
) -> Result<Option<i32>, CategoryError> {
    let mut parent: Option<i32> = None;
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(None);
    }
    for part in trimmed.split('/') {
        use crate::schema::news_bundles::dsl as b;
        let found = b::news_bundles
            .filter(b::name.eq(part))
            .filter(b::parent_bundle_id.eq(parent))
            .first::<crate::models::Bundle>(conn)
            .await
            .optional()?;
        match found {
            Some(bun) => parent = Some(bun.id),
            None => return Err(CategoryError::InvalidPath),
        }
    }
    Ok(parent)
}

pub async fn list_names_at_path(
    conn: &mut DbConnection,
    path: Option<&str>,
) -> Result<Vec<String>, CategoryError> {
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

pub async fn get_article(
    conn: &mut DbConnection,
    path: &str,
    article_id: i32,
) -> Result<Option<crate::models::Article>, CategoryError> {
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

async fn category_id_from_path(conn: &mut DbConnection, path: &str) -> Result<i32, CategoryError> {
    use diesel::sql_types::{Integer, Nullable};
    use diesel_cte_ext::with_recursive;

    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Err(CategoryError::InvalidPath);
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    let seg_values = parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "SELECT {} AS idx, '{}' AS name",
                i + 1,
                p.replace('\'', "''")
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let len = parts.len();

    let seed = diesel::dsl::sql::<(Integer, Nullable<Integer>)>("SELECT 0, NULL");
    let step_sql = format!(
        "SELECT tree.idx + 1, b.id FROM tree \
         JOIN ({values}) AS seg ON seg.idx = tree.idx + 1 \
         LEFT JOIN news_bundles b ON b.name = seg.name AND ((tree.id IS NULL AND b.parent_bundle_id IS NULL) OR b.parent_bundle_id = tree.id)",
        values = seg_values
    );
    let step = diesel::dsl::sql::<(Integer, Nullable<Integer>)>(&step_sql);
    let body_sql = format!(
        "SELECT id FROM news_categories WHERE name = (SELECT name FROM ({values}) AS seg WHERE idx = {last}) AND bundle_id IS (SELECT id FROM tree WHERE idx = {parent})",
        values = seg_values,
        last = len,
        parent = len - 1
    );
    let body = diesel::dsl::sql::<Integer>(&body_sql);
    let query =
        with_recursive::<diesel::sqlite::Sqlite, _, _, _>("tree", &["idx", "id"], seed, step, body);
    let res: Option<i32> = query.get_result(conn).await.optional()?;
    res.ok_or(CategoryError::InvalidPath)
}

pub async fn list_article_titles(
    conn: &mut DbConnection,
    path: &str,
) -> Result<Vec<String>, CategoryError> {
    let cat_id = category_id_from_path(conn, path).await?;
    use crate::schema::news_articles::dsl as a;
    let titles = a::news_articles
        .filter(a::category_id.eq(cat_id))
        .order(a::posted_at.asc())
        .select(a::title)
        .load::<String>(conn)
        .await
        .map_err(CategoryError::Diesel)?;
    Ok(titles)
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
