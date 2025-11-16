//! File metadata helpers.

use diesel::{prelude::*, result::QueryResult};
use diesel_async::RunQueryDsl;

use super::connection::DbConnection;

/// Insert a new file metadata entry.
///
/// # Errors
/// Returns any error produced by the database.
#[must_use = "handle the result"]
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
#[must_use = "handle the result"]
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
#[must_use = "handle the result"]
pub async fn list_files_for_user(
    conn: &mut DbConnection,
    uid: i32,
) -> QueryResult<Vec<crate::models::FileEntry>> {
    use crate::schema::{file_acl::dsl as a, files::dsl as f};
    f::files
        .inner_join(a::file_acl.on(a::file_id.eq(f::id)))
        .filter(a::user_id.eq(uid))
        .order(f::name.asc())
        .select((f::id, f::name, f::object_key, f::size))
        .load::<crate::models::FileEntry>(conn)
        .await
}
