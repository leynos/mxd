use chrono::NaiveDateTime;
use crate::schema::{file_acl, files};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub password: String,
}

#[derive(Insertable, Deserialize)]
#[diesel(table_name = crate::schema::users)]
pub struct NewUser<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub bundle_id: Option<i32>,
}

#[derive(Insertable, Deserialize)]
#[diesel(table_name = crate::schema::news_categories)]
pub struct NewCategory<'a> {
    pub name: &'a str,
    pub bundle_id: Option<i32>,
}

#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct Bundle {
    pub id: i32,
    pub parent_bundle_id: Option<i32>,
    pub name: String,
}

#[derive(Insertable, Deserialize)]
#[diesel(table_name = crate::schema::news_bundles)]
pub struct NewBundle<'a> {
    pub parent_bundle_id: Option<i32>,
    pub name: &'a str,
}

#[derive(Queryable, Debug)]
pub struct Article {
    pub id: i32,
    pub category_id: i32,
    pub parent_article_id: Option<i32>,
    pub prev_article_id: Option<i32>,
    pub next_article_id: Option<i32>,
    pub first_child_article_id: Option<i32>,
    pub title: String,
    pub poster: Option<String>,
    pub posted_at: NaiveDateTime,
    pub flags: i32,
    pub data_flavor: Option<String>,
    pub data: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::news_articles)]
pub struct NewArticle<'a> {
    pub category_id: i32,
    pub parent_article_id: Option<i32>,
    pub prev_article_id: Option<i32>,
    pub next_article_id: Option<i32>,
    pub first_child_article_id: Option<i32>,
    pub title: &'a str,
    pub poster: Option<&'a str>,
    pub posted_at: NaiveDateTime,
    pub flags: i32,
    pub data_flavor: Option<&'a str>,
    pub data: Option<&'a str>,
}

#[derive(Queryable, Serialize, Deserialize, Debug)]
pub struct FileEntry {
    pub id: i32,
    pub name: String,
    pub object_key: String,
    pub size: i64,
}

#[derive(Insertable)]
#[diesel(table_name = files)]
pub struct NewFileEntry<'a> {
    pub name: &'a str,
    pub object_key: &'a str,
    pub size: i64,
}

#[derive(Insertable)]
#[diesel(table_name = file_acl)]
pub struct NewFileAcl {
    pub file_id: i32,
    pub user_id: i32,
}
