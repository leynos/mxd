//! User record helpers.

use diesel::{prelude::*, result::QueryResult};
use diesel_async::RunQueryDsl;

use super::connection::DbConnection;

/// Look up a user record by username.
///
/// # Errors
/// Returns any error produced by the underlying database query.
#[must_use = "handle the result"]
pub async fn get_user_by_name(
    conn: &mut DbConnection,
    name: &str,
) -> QueryResult<Option<crate::models::User>> {
    use crate::schema::users::dsl::{username, users};
    users
        .filter(username.eq(name))
        .first::<crate::models::User>(conn)
        .await
        .optional()
}

/// Insert a new user record.
///
/// # Errors
/// Returns any error produced by the insertion query.
#[must_use = "handle the result"]
pub async fn create_user(
    conn: &mut DbConnection,
    user: &crate::models::NewUser<'_>,
) -> QueryResult<usize> {
    use crate::schema::users::dsl::users;
    diesel::insert_into(users).values(user).execute(conn).await
}
