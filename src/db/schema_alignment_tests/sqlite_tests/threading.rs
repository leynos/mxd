//! `SQLite` article-threading schema behaviour tests.

#![cfg(feature = "sqlite")]

use anyhow::Context;
use diesel_async::RunQueryDsl;
use rstest::rstest;

use super::{
    super::{DbConnection, NameRow, TestResult},
    common::threaded_articles_db,
};

#[rstest]
#[tokio::test]
async fn sqlite_article_threading_enforces_referential_integrity(
    #[future] threaded_articles_db: TestResult<DbConnection>,
) -> TestResult<()> {
    let mut conn = threaded_articles_db.await?;

    // Verify the threading link via a JOIN query
    let linked = diesel::sql_query(
        "SELECT child.id AS name FROM news_articles a INNER JOIN news_articles child ON child.id \
         = a.first_child_article_id WHERE a.id = 1",
    )
    .get_result::<NameRow>(&mut conn)
    .await
    .context("LOAD SQLite linked child article")?;
    anyhow::ensure!(linked.name == "2", "root article must link to its child");

    // Referential integrity: inserting an article with a non-existent parent must fail
    // (PRAGMA foreign_keys must be ON; SQLite pragmas are connection-scoped)
    diesel::sql_query("PRAGMA foreign_keys = ON")
        .execute(&mut conn)
        .await
        .context("EXECUTE enable SQLite foreign keys")?;
    let bad_insert = diesel::sql_query(
        "INSERT INTO news_articles (id, category_id, parent_article_id, title, posted_at) VALUES \
         (99, 1, 9999, 'Orphan', '2026-01-03 00:00:00')",
    )
    .execute(&mut conn)
    .await;
    let bad_insert_error = match bad_insert {
        Ok(_) => anyhow::bail!(
            "insert with non-existent parent_article_id must be rejected when FK enforcement is on"
        ),
        Err(error) => error.to_string(),
    };
    anyhow::ensure!(
        bad_insert_error.contains("FOREIGN KEY constraint failed"),
        "insert with non-existent parent_article_id must fail with a FOREIGN KEY constraint \
         error; got: {bad_insert_error}"
    );
    let bad_update =
        diesel::sql_query("UPDATE news_articles SET first_child_article_id = 9999 WHERE id = 1")
            .execute(&mut conn)
            .await;
    let bad_update_error = match bad_update {
        Ok(_) => anyhow::bail!(
            "update with non-existent first_child_article_id must be rejected when FK enforcement \
             is on"
        ),
        Err(error) => error.to_string(),
    };
    anyhow::ensure!(
        bad_update_error.contains("FOREIGN KEY constraint failed"),
        "update with non-existent first_child_article_id must fail with a FOREIGN KEY constraint \
         error; got: {bad_update_error}"
    );
    Ok(())
}
