//! Diesel ORM models for persisted data.
//!
//! These structs correspond to tables defined in `schema.rs` and are used
//! throughout the application for reading and writing user, news, and file
//! sharing records.

use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::schema::{
    file_nodes,
    groups,
    permissions,
    resource_permissions,
    user_groups,
    user_permissions,
};

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

/// File-node kinds stored in the shared file hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileNodeKind {
    /// A stored file with an object key and byte size.
    File,
    /// A folder that can hold child nodes.
    Folder,
    /// An alias that points at another file node.
    Alias,
}

impl FileNodeKind {
    /// Return the database representation for this node kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Folder => "folder",
            Self::Alias => "alias",
        }
    }
}

/// Represents one permission in the shared privilege catalogue.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct Permission {
    /// Unique permission identifier.
    pub id: i32,
    /// Protocol privilege code associated with this permission.
    pub code: i32,
    /// Stable symbolic name for the permission.
    pub name: String,
    /// Human-readable description for operators and developers.
    pub description: String,
}

/// Parameters for inserting or seeding a permission row.
#[derive(Insertable)]
#[diesel(table_name = permissions)]
pub struct NewPermission<'a> {
    /// Protocol privilege code associated with this permission.
    pub code: i32,
    /// Stable symbolic name for the permission.
    pub name: &'a str,
    /// Human-readable description for the permission.
    pub description: &'a str,
}

/// Represents a group principal used by resource ACLs.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct Group {
    /// Unique group identifier.
    pub id: i32,
    /// Unique group name.
    pub name: String,
}

/// Parameters for creating a new group principal.
#[derive(Insertable)]
#[diesel(table_name = groups)]
pub struct NewGroup<'a> {
    /// Unique group name.
    pub name: &'a str,
}

/// Parameters for linking a user to a group.
#[derive(Insertable)]
#[diesel(table_name = user_groups)]
pub struct NewUserGroup {
    /// User being added to the group.
    pub user_id: i32,
    /// Group receiving the user.
    pub group_id: i32,
}

/// Parameters for granting a global permission to a user.
#[derive(Insertable)]
#[diesel(table_name = user_permissions)]
pub struct NewUserPermission {
    /// User receiving the permission.
    pub user_id: i32,
    /// Permission being granted.
    pub permission_id: i32,
}

/// Represents one node in the shared file hierarchy.
#[derive(Queryable, Debug)]
pub struct FileNode {
    /// Unique file-node identifier.
    pub id: i32,
    /// Stored kind string (`file`, `folder`, or `alias`).
    pub kind: String,
    /// Basename for this node inside its parent folder.
    pub name: String,
    /// Parent folder identifier, or `None` for a top-level node.
    pub parent_id: Option<i32>,
    /// Alias target identifier when this node is an alias.
    pub alias_target_id: Option<i32>,
    /// Object storage key for file nodes.
    pub object_key: Option<String>,
    /// File size in bytes for file nodes.
    pub size: Option<i64>,
    /// User-visible comment attached to this node.
    pub comment: Option<String>,
    /// Whether this folder acts as a drop box.
    pub is_dropbox: bool,
    /// Identifier of the user who created the node.
    pub creator_id: i32,
    /// Timestamp when the node was created.
    pub created_at: NaiveDateTime,
    /// Timestamp when the node was last updated.
    pub updated_at: NaiveDateTime,
}

/// Parameters for inserting a new file node.
#[derive(Insertable)]
#[diesel(table_name = file_nodes)]
pub struct NewFileNode<'a> {
    /// Stored kind string (`file`, `folder`, or `alias`).
    pub kind: &'a str,
    /// Basename for this node inside its parent folder.
    pub name: &'a str,
    /// Parent folder identifier, or `None` for a top-level node.
    pub parent_id: Option<i32>,
    /// Alias target identifier when this node is an alias.
    pub alias_target_id: Option<i32>,
    /// Object storage key for file nodes.
    pub object_key: Option<&'a str>,
    /// File size in bytes for file nodes.
    pub size: Option<i64>,
    /// User-visible comment attached to this node.
    pub comment: Option<&'a str>,
    /// Whether this folder acts as a drop box.
    pub is_dropbox: bool,
    /// Identifier of the user who created the node.
    pub creator_id: i32,
}

/// Minimal projection used for visible file-list queries.
#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct VisibleFileNode {
    /// Unique file-node identifier.
    pub id: i32,
    /// Basename shown to the client.
    pub name: String,
    /// Stored kind string (`file`, `folder`, or `alias`).
    pub kind: String,
}

/// Parameters for granting a resource-scoped permission.
#[derive(Insertable)]
#[diesel(table_name = resource_permissions)]
pub struct NewResourcePermission<'a> {
    /// Resource type protected by the ACL row.
    pub resource_type: &'a str,
    /// Identifier of the protected resource.
    pub resource_id: i32,
    /// Principal type (`user` or `group`).
    pub principal_type: &'a str,
    /// Identifier of the principal receiving the permission.
    pub principal_id: i32,
    /// Permission being granted on the resource.
    pub permission_id: i32,
}
