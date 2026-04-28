//! Legacy `files`/`file_acl` visibility query support.

use diesel::{QueryResult, Queryable, prelude::*};
use diesel_async::RunQueryDsl;

use crate::{
    db::DbConnection,
    models::VisibleFileNode,
    schema::{file_acl::dsl as file_acl, files::dsl as files},
};

/// List legacy root files visible to `user_id` through `file_acl` rows.
///
/// Joins legacy `files` records to explicit legacy ACL grants for the selected
/// user, orders visible files by name, and maps each result into a
/// `VisibleFileNode` with `kind = "file"` for merge compatibility with modern
/// `file_nodes` visibility queries.
///
/// # Errors
///
/// Returns `Err` if the database connection or visibility query fails.
pub(super) async fn list_legacy_visible_root_files_for_user(
    conn: &mut DbConnection,
    user_id: i32,
) -> QueryResult<Vec<VisibleFileNode>> {
    #[derive(Queryable)]
    struct LegacyVisibleFile {
        id: i32,
        name: String,
    }

    let legacy_files = files::files
        .inner_join(file_acl::file_acl.on(file_acl::file_id.eq(files::id)))
        .filter(file_acl::user_id.eq(user_id))
        .order(files::name.asc())
        .select((files::id, files::name))
        .load::<LegacyVisibleFile>(conn)
        .await?;

    Ok(legacy_files
        .into_iter()
        .map(|file| VisibleFileNode {
            id: file.id,
            name: file.name,
            kind: String::from("file"),
        })
        .collect())
}
