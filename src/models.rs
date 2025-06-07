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
