use chrono::{DateTime, Utc};
use diesel_async::{AsyncConnection, RunQueryDsl};
use futures_util::future::BoxFuture;
use mxd::{
    db::{
        DbConnection,
        add_file_acl,
        apply_migrations,
        create_bundle,
        create_category,
        create_file,
        create_user,
    },
    models::{NewArticle, NewBundle, NewCategory, NewFileAcl, NewFileEntry, NewUser},
    users::hash_password,
};

use crate::AnyError;

pub fn with_db<F>(db: &str, f: F) -> Result<(), AnyError>
where
    F: for<'c> FnOnce(&'c mut DbConnection) -> BoxFuture<'c, Result<(), AnyError>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut conn = DbConnection::establish(db).await?;
        apply_migrations(&mut conn, db).await?;
        f(&mut conn).await
    })
}

pub fn setup_files_db(db: &str) -> Result<(), AnyError> {
    with_db(db, |conn| {
        Box::pin(async move {
            let argon2 = argon2::Argon2::default();
            let hashed = hash_password(&argon2, "secret")?;
            let new_user = NewUser {
                username: "alice",
                password: &hashed,
            };
            create_user(conn, &new_user).await?;
            let files = [
                NewFileEntry {
                    name: "fileA.txt",
                    object_key: "1",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileB.txt",
                    object_key: "2",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileC.txt",
                    object_key: "3",
                    size: 1,
                },
            ];
            for file in &files {
                create_file(conn, file).await?;
            }
            let acls = [
                NewFileAcl {
                    file_id: 1,
                    user_id: 1,
                },
                NewFileAcl {
                    file_id: 3,
                    user_id: 1,
                },
            ];
            for acl in &acls {
                add_file_acl(conn, acl).await?;
            }
            Ok(())
        })
    })
}

pub fn setup_news_db(db: &str) -> Result<(), AnyError> {
    with_db(db, |conn| {
        Box::pin(async move {
            create_category(
                conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            use mxd::schema::news_articles::dsl as a;
            let posted = DateTime::<Utc>::from_timestamp(1000, 0)
                .expect("valid timestamp")
                .naive_utc();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: None,
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "First",
                    poster: None,
                    posted_at: posted,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("a"),
                })
                .execute(conn)
                .await?;
            let posted2 = DateTime::<Utc>::from_timestamp(2000, 0)
                .expect("valid timestamp")
                .naive_utc();
            diesel::insert_into(a::news_articles)
                .values(&NewArticle {
                    category_id: 1,
                    parent_article_id: None,
                    prev_article_id: Some(1),
                    next_article_id: None,
                    first_child_article_id: None,
                    title: "Second",
                    poster: None,
                    posted_at: posted2,
                    flags: 0,
                    data_flavor: Some("text/plain"),
                    data: Some("b"),
                })
                .execute(conn)
                .await?;
            Ok(())
        })
    })
}

pub fn setup_news_categories_root_db(db: &str) -> Result<(), AnyError> {
    setup_news_categories_with_structure(db, |conn, _| {
        Box::pin(async move {
            create_category(
                conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            create_category(
                conn,
                &NewCategory {
                    name: "Updates",
                    bundle_id: None,
                },
            )
            .await?;
            Ok(())
        })
    })
}

pub fn setup_news_categories_nested_db(db: &str) -> Result<(), AnyError> {
    setup_news_categories_with_structure(db, |conn, root_id| {
        Box::pin(async move {
            let sub_id = create_bundle(
                conn,
                &NewBundle {
                    parent_bundle_id: Some(root_id),
                    name: "Sub",
                },
            )
            .await?;

            create_category(
                conn,
                &NewCategory {
                    name: "Inside",
                    bundle_id: Some(sub_id),
                },
            )
            .await?;
            Ok(())
        })
    })
}

pub fn setup_news_categories_with_structure<F>(db: &str, build: F) -> Result<(), AnyError>
where
    F: Send
        + 'static
        + for<'c> FnOnce(&'c mut DbConnection, i32) -> BoxFuture<'c, Result<(), AnyError>>,
{
    with_db(db, |conn| {
        Box::pin(async move {
            let root_id = insert_root_bundle(conn).await?;
            build(conn, root_id).await
        })
    })
}

async fn insert_root_bundle(conn: &mut DbConnection) -> Result<i32, AnyError> {
    let id = create_bundle(
        conn,
        &NewBundle {
            parent_bundle_id: None,
            name: "Bundle",
        },
    )
    .await?;

    Ok(id)
}
