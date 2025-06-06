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

pub async fn get_all_categories(
    conn: &mut DbConnection,
    path: Option<&str>,
) -> QueryResult<Vec<crate::models::Category>> {
    if let Some(p) = path {
        if p != "/" {
            // Path-based filtering isn't implemented yet, so treat any
            // non-root path as missing and report `NotFound`.
            return Err(diesel::result::Error::NotFound);
        }
    }
    use crate::schema::news_categories::dsl::*;
    news_categories
        .order(name.asc())
        .load::<crate::models::Category>(conn)
        .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NewCategory, NewUser};
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

    #[tokio::test]
    async fn test_create_and_get_category() {
        let mut conn = DbConnection::establish(":memory:").await.unwrap();
        run_migrations(&mut conn).await.unwrap();
        let cat = NewCategory { name: "General" };
        create_category(&mut conn, &cat).await.unwrap();
        let cats = get_all_categories(&mut conn, None).await.unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "General");
    }
}
