//! Shared `SQLite` schema-alignment fixtures and catalogue readers.

#![cfg(feature = "sqlite")]

use anyhow::Context;
use diesel::sql_query;
use diesel_async::{AsyncConnection, RunQueryDsl};
use rstest::fixture;

use super::super::{DbConnection, NameRow, TestResult, apply_migrations, setup_legacy_schema};

pub(super) async fn sqlite_conn() -> TestResult<DbConnection> {
    let mut conn = DbConnection::establish(":memory:").await?;
    apply_migrations(&mut conn, "", None).await?;
    Ok(conn)
}

#[fixture]
pub(super) async fn two_bundle_db() -> TestResult<DbConnection> {
    let mut conn = sqlite_conn().await?;
    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (1, NULL, 'A')",
    )
    .execute(&mut conn)
    .await
    .context("EXECUTE insert first SQLite test bundle")?;
    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (2, NULL, 'B')",
    )
    .execute(&mut conn)
    .await
    .context("EXECUTE insert second SQLite test bundle")?;
    Ok(conn)
}

#[fixture]
pub(super) async fn add_sn_db(
    #[future] two_bundle_db: TestResult<DbConnection>,
) -> TestResult<DbConnection> {
    let mut conn = two_bundle_db.await?;
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'WithTwo', 1)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert SQLite add_sn populated category")?;
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (2, 'Empty', 1)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert SQLite add_sn empty category")?;

    for i in 1_i32..=2 {
        diesel::sql_query(format!(
            "INSERT INTO news_articles (id, category_id, title, posted_at) VALUES ({i}, 1, \
             'Article {i}', '2026-01-01 00:00:00')"
        ))
        .execute(&mut conn)
        .await
        .context("EXECUTE insert SQLite add_sn article")?;
    }
    Ok(conn)
}

#[fixture]
pub(super) async fn threaded_articles_db() -> TestResult<DbConnection> {
    let mut conn = sqlite_conn().await?;
    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (1, NULL, 'ThreadBundle')",
    )
    .execute(&mut conn)
    .await
    .context("EXECUTE insert SQLite threading bundle")?;
    diesel::sql_query(
        "INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'ThreadCat', 1)",
    )
    .execute(&mut conn)
    .await
    .context("EXECUTE insert SQLite threading category")?;

    diesel::sql_query(
        "INSERT INTO news_articles (id, category_id, parent_article_id, prev_article_id, \
         next_article_id, first_child_article_id, title, posted_at) VALUES (1, 1, NULL, NULL, \
         NULL, NULL, 'Root', '2026-01-01 00:00:00')",
    )
    .execute(&mut conn)
    .await
    .context("EXECUTE insert SQLite root threading article")?;
    diesel::sql_query(
        "INSERT INTO news_articles (id, category_id, parent_article_id, prev_article_id, \
         next_article_id, first_child_article_id, title, posted_at) VALUES (2, 1, 1, NULL, NULL, \
         NULL, 'Child', '2026-01-02 00:00:00')",
    )
    .execute(&mut conn)
    .await
    .context("EXECUTE insert SQLite child threading article")?;
    diesel::sql_query("UPDATE news_articles SET first_child_article_id = 2 WHERE id = 1")
        .execute(&mut conn)
        .await
        .context("EXECUTE link SQLite root article to child")?;
    Ok(conn)
}

pub(super) async fn setup_sqlite_legacy_schema(conn: &mut DbConnection) -> TestResult<()> {
    setup_legacy_schema(
        conn,
        &[
            include_str!("../../../../migrations/sqlite/00000000000000_create_users/up.sql"),
            include_str!("../../../../migrations/sqlite/00000000000001_create_news/up.sql"),
            include_str!("../../../../migrations/sqlite/00000000000002_add_bundles/up.sql"),
            include_str!("../../../../migrations/sqlite/00000000000003_add_articles/up.sql"),
            include_str!("../../../../migrations/sqlite/00000000000004_create_files/up.sql"),
            include_str!(
                "../../../../migrations/sqlite/00000000000005_add_bundle_name_parent_index/up.sql"
            ),
        ],
    )
    .await
}

pub(super) async fn sqlite_names(conn: &mut DbConnection, sql: &str) -> TestResult<Vec<String>> {
    Ok(sql_query(sql)
        .load::<NameRow>(conn)
        .await
        .with_context(|| format!("LOAD SQLite names: {sql}"))?
        .into_iter()
        .map(|row| row.name)
        .collect())
}
