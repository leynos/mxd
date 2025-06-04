use diesel::prelude::*;
use diesel::r2d2::{self, ConnectionManager};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use std::ops::Deref;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub type DbPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

pub fn establish_pool(database_url: &str) -> DbPool {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool")
}

pub fn run_migrations(conn: &mut SqliteConnection) {
    conn.run_pending_migrations(MIGRATIONS).expect("migrations failed");
}

pub struct Connection(pub r2d2::PooledConnection<ConnectionManager<SqliteConnection>>);

impl Deref for Connection {
    type Target = SqliteConnection;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn get_user_by_name(conn: &mut SqliteConnection, name: &str) -> QueryResult<Option<crate::models::User>> {
    use crate::schema::users::dsl::*;
    users.filter(username.eq(name)).first::<crate::models::User>(conn).optional()
}

pub fn create_user(conn: &mut SqliteConnection, user: &crate::models::NewUser<'_>) -> QueryResult<usize> {
    use crate::schema::users::dsl::*;
    diesel::insert_into(users).values(user).execute(conn)
}
