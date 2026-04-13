//! Diesel ORM models for persisted data.
//!
//! These structs correspond to tables defined in `schema.rs` and are used
//! throughout the application for reading and writing user and news records.
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::schema::{file_acl, file_nodes, files, resource_permissions, user_permissions};

/// Represents a user account stored in the database.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct User {
    /// Unique user identifier.
    pub id: i32,
    /// Username for login.
    pub username: String,
    /// Hashed password.
    pub password: String,
}

/// Parameters for creating a new user account.
#[derive(Insertable, Deserialize)]
#[diesel(table_name = crate::schema::users)]
pub struct NewUser<'a> {
    /// Username for login.
    pub username: &'a str,
    /// Hashed password.
    pub password: &'a str,
}

/// Represents a news category in the database.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct Category {
    /// Unique category identifier.
    pub id: i32,
    /// Category name.
    pub name: String,
    /// Parent bundle identifier, if any.
    pub bundle_id: Option<i32>,
}

/// Parameters for creating a new news category.
#[derive(Insertable, Deserialize)]
#[diesel(table_name = crate::schema::news_categories)]
pub struct NewCategory<'a> {
    /// Category name.
    pub name: &'a str,
    /// Parent bundle identifier, if any.
    pub bundle_id: Option<i32>,
}

/// Represents a news bundle (grouping of categories) in the database.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct Bundle {
    /// Unique bundle identifier.
    pub id: i32,
    /// Parent bundle identifier for nested bundles.
    pub parent_bundle_id: Option<i32>,
    /// Bundle name.
    pub name: String,
}

/// Parameters for creating a new news bundle.
#[derive(Insertable, Deserialize)]
#[diesel(table_name = crate::schema::news_bundles)]
pub struct NewBundle<'a> {
    /// Parent bundle identifier for nested bundles.
    pub parent_bundle_id: Option<i32>,
    /// Bundle name.
    pub name: &'a str,
}

/// Represents a news article stored in the database.
#[derive(Queryable, Debug)]
pub struct Article {
    /// Unique article identifier.
    pub id: i32,
    /// Category containing this article.
    pub category_id: i32,
    /// Parent article identifier for threaded replies.
    pub parent_article_id: Option<i32>,
    /// Previous sibling article identifier.
    pub prev_article_id: Option<i32>,
    /// Next sibling article identifier.
    pub next_article_id: Option<i32>,
    /// First child article identifier for threaded replies.
    pub first_child_article_id: Option<i32>,
    /// Article title.
    pub title: String,
    /// Username of the article's author.
    pub poster: Option<String>,
    /// Timestamp when the article was posted.
    pub posted_at: NaiveDateTime,
    /// Article flags.
    pub flags: i32,
    /// Content type of the article data.
    pub data_flavor: Option<String>,
    /// Article content.
    pub data: Option<String>,
}

/// Parameters for creating a new news article.
#[derive(Insertable)]
#[diesel(table_name = crate::schema::news_articles)]
pub struct NewArticle<'a> {
    /// Category containing this article.
    pub category_id: i32,
    /// Parent article identifier for threaded replies.
    pub parent_article_id: Option<i32>,
    /// Previous sibling article identifier.
    pub prev_article_id: Option<i32>,
    /// Next sibling article identifier.
    pub next_article_id: Option<i32>,
    /// First child article identifier for threaded replies.
    pub first_child_article_id: Option<i32>,
    /// Article title.
    pub title: &'a str,
    /// Username of the article's author.
    pub poster: Option<&'a str>,
    /// Timestamp when the article was posted.
    pub posted_at: NaiveDateTime,
    /// Article flags.
    pub flags: i32,
    /// Content type of the article data.
    pub data_flavor: Option<&'a str>,
    /// Article content.
    pub data: Option<&'a str>,
}

/// Represents a file entry in the database.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct FileEntry {
    /// Unique file identifier.
    pub id: i32,
    /// File name as shown to clients.
    pub name: String,
    /// Object storage key for retrieving the file.
    pub object_key: String,
    /// File size in bytes.
    pub size: i64,
}

/// Parameters for creating a new file entry.
#[derive(Insertable)]
#[diesel(table_name = files)]
pub struct NewFileEntry<'a> {
    /// File name as shown to clients.
    pub name: &'a str,
    /// Object storage key for retrieving the file.
    pub object_key: &'a str,
    /// File size in bytes.
    pub size: i64,
}

/// Parameters for creating a file access control entry.
#[derive(Insertable)]
#[diesel(table_name = file_acl)]
pub struct NewFileAcl {
    /// File being granted access to.
    pub file_id: i32,
    /// User being granted access.
    pub user_id: i32,
}

/// Shared global permission definition.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct PermissionDefinition {
    /// Unique permission identifier.
    pub id: i32,
    /// Hotline privilege bit position.
    pub code: i32,
    /// Stable symbolic permission name.
    pub name: String,
    /// Scope label for the permission family.
    pub scope: String,
}

/// Link from a user to a shared global permission definition.
#[derive(Insertable, Queryable, Serialize, Deserialize, Debug)]
#[diesel(table_name = user_permissions)]
pub struct UserPermission {
    /// User receiving the permission.
    pub user_id: i32,
    /// Shared permission definition being granted.
    pub permission_id: i32,
}

/// A hierarchical file tree node.
#[derive(Queryable, Debug)]
pub struct FileNode {
    /// Unique file-node identifier.
    pub id: i32,
    /// Whether this row is the sentinel root folder.
    pub is_root: bool,
    /// Node kind (`file`, `folder`, or `alias`).
    pub node_type: String,
    /// Visible name within the parent folder.
    pub name: String,
    /// Parent folder identifier, or `None` for the sentinel root.
    pub parent_id: Option<i32>,
    /// Alias target when `node_type` is `alias`.
    pub alias_target_id: Option<i32>,
    /// Object storage key for file content.
    pub object_key: Option<String>,
    /// Stored file size in bytes.
    pub size: i64,
    /// Optional user-visible comment.
    pub comment: Option<String>,
    /// Whether the folder behaves like a drop box.
    pub is_dropbox: bool,
    /// Creation timestamp.
    pub created_at: NaiveDateTime,
    /// Last update timestamp.
    pub updated_at: NaiveDateTime,
    /// User who created the node.
    pub created_by: Option<i32>,
}

/// Parameters for creating a new file tree node.
#[derive(Insertable)]
#[diesel(table_name = file_nodes)]
pub struct NewFileNode<'a> {
    /// Whether the row is the sentinel root folder.
    pub is_root: bool,
    /// Node kind (`file`, `folder`, or `alias`).
    pub node_type: &'a str,
    /// Visible name within the parent folder.
    pub name: &'a str,
    /// Parent folder identifier, or `None` for the root.
    pub parent_id: Option<i32>,
    /// Alias target when `node_type` is `alias`.
    pub alias_target_id: Option<i32>,
    /// Object storage key for file content.
    pub object_key: Option<&'a str>,
    /// Stored file size in bytes.
    pub size: i64,
    /// Optional user-visible comment.
    pub comment: Option<&'a str>,
    /// Whether the folder behaves like a drop box.
    pub is_dropbox: bool,
    /// User who created the node.
    pub created_by: Option<i32>,
}

/// A resource-scoped permission grant for a file node.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct ResourcePermission {
    /// Unique grant identifier.
    pub id: i32,
    /// Resource family label.
    pub resource_type: String,
    /// Granted resource identifier.
    pub resource_id: i32,
    /// Principal family label.
    pub principal_type: String,
    /// Principal identifier.
    pub principal_id: i32,
    /// Granted Hotline privilege bitmask.
    pub privileges: i64,
}

/// Parameters for creating a resource-scoped permission grant.
#[derive(Insertable)]
#[diesel(table_name = resource_permissions)]
pub struct NewResourcePermission<'a> {
    /// Resource family label.
    pub resource_type: &'a str,
    /// Granted resource identifier.
    pub resource_id: i32,
    /// Principal family label.
    pub principal_type: &'a str,
    /// Principal identifier.
    pub principal_id: i32,
    /// Granted Hotline privilege bitmask.
    pub privileges: i64,
}
