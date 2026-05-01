//! `PostgreSQL` article-threading schema behaviour tests for roadmap item 4.1.1.
//!
//! Verifies that the `news_articles` self-referential threading columns
//! (`parent_article_id`, `first_child_article_id`) support valid parent-child
//! relationships via JOIN queries and that foreign-key constraints reject
//! references to non-existent articles.  Tests run against an embedded
//! `PostgreSQL` cluster and are serialised via `serial_test::file_serial`.

use diesel::sql_query;
use diesel_async::{AsyncConnection, RunQueryDsl};

use super::{
    super::{DbConnection, TestResult, apply_migrations},
    catalogue_helpers::{postgres_names, with_postgres_test_db},
};

struct ThreadSeedIds {
    category: String,
    root_article: String,
    child_article: String,
}

async fn seed_bundle_and_category(conn: &mut DbConnection) -> TestResult<String> {
    sql_query("INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'ThreadBundle')")
        .execute(conn)
        .await?;

    let bundle_ids = postgres_names(
        conn,
        "SELECT id::text AS name FROM news_bundles ORDER BY id",
    )
    .await?;
    let bid = bundle_ids
        .as_slice()
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing seeded bundle id for threading test"))?;

    sql_query(format!(
        "INSERT INTO news_categories (name, bundle_id) VALUES ('ThreadCat', {bid})"
    ))
    .execute(conn)
    .await?;

    let cat_ids = postgres_names(
        conn,
        "SELECT id::text AS name FROM news_categories ORDER BY id",
    )
    .await?;
    cat_ids
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing seeded category id for threading test"))
}

async fn insert_root_and_child(
    conn: &mut DbConnection,
    category_id: &str,
) -> TestResult<ThreadSeedIds> {
    sql_query(format!(
        "INSERT INTO news_articles (category_id, parent_article_id, prev_article_id, \
         next_article_id, first_child_article_id, title, posted_at) VALUES ({category_id}, NULL, \
         NULL, NULL, NULL, 'Root', NOW())"
    ))
    .execute(conn)
    .await?;

    let root_ids = postgres_names(
        conn,
        "SELECT id::text AS name FROM news_articles ORDER BY id",
    )
    .await?;
    let rid = root_ids
        .as_slice()
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing seeded root article id for threading test"))?;

    sql_query(format!(
        "INSERT INTO news_articles (category_id, parent_article_id, prev_article_id, \
         next_article_id, first_child_article_id, title, posted_at) VALUES ({category_id}, {rid}, \
         NULL, NULL, NULL, 'Child', NOW())"
    ))
    .execute(conn)
    .await?;

    let child_ids = postgres_names(
        conn,
        "SELECT id::text AS name FROM news_articles WHERE parent_article_id IS NOT NULL ORDER BY \
         id",
    )
    .await?;
    anyhow::ensure!(child_ids.len() == 1, "expected one child article");
    let child_article = child_ids
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing seeded child article id for threading test"))?;

    Ok(ThreadSeedIds {
        category: category_id.to_owned(),
        root_article: rid.clone(),
        child_article,
    })
}

async fn link_root_to_child(conn: &mut DbConnection, ids: &ThreadSeedIds) -> TestResult<()> {
    let rid = &ids.root_article;
    let chid = &ids.child_article;
    sql_query(format!(
        "UPDATE news_articles SET first_child_article_id = {chid} WHERE id = {rid}"
    ))
    .execute(conn)
    .await?;
    Ok(())
}

async fn assert_threading_integrity(
    conn: &mut DbConnection,
    ids: &ThreadSeedIds,
) -> TestResult<()> {
    let rid = &ids.root_article;
    let chid = &ids.child_article;
    let linked = postgres_names(
        conn,
        &format!(
            "SELECT child.id::text AS name FROM news_articles a INNER JOIN news_articles child ON \
             child.id = a.first_child_article_id WHERE a.id = {rid}"
        ),
    )
    .await?;
    anyhow::ensure!(linked.len() == 1, "root article must link to its child");
    anyhow::ensure!(linked[0] == *chid, "linked child id must match");
    Ok(())
}

async fn assert_missing_references_are_rejected(
    conn: &mut DbConnection,
    ids: &ThreadSeedIds,
) -> TestResult<()> {
    let category_id = &ids.category;
    let bad_insert = sql_query(format!(
        "INSERT INTO news_articles (category_id, parent_article_id, title, posted_at) VALUES \
         ({category_id}, 999999, 'Orphan', NOW())"
    ))
    .execute(conn)
    .await;
    anyhow::ensure!(
        bad_insert.is_err(),
        "insert with non-existent parent_article_id must be rejected"
    );
    let rid = &ids.root_article;
    let bad_update = sql_query(format!(
        "UPDATE news_articles SET first_child_article_id = 999999 WHERE id = {rid}"
    ))
    .execute(conn)
    .await;
    anyhow::ensure!(
        bad_update.is_err(),
        "update with non-existent first_child_article_id must be rejected"
    );
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_article_threading_enforces_referential_integrity() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        let category_id = seed_bundle_and_category(&mut conn).await?;
        let ids = insert_root_and_child(&mut conn, &category_id).await?;
        assert!(
            !ids.root_article.is_empty(),
            "root article id must be captured"
        );
        link_root_to_child(&mut conn, &ids).await?;
        assert_threading_integrity(&mut conn, &ids).await?;
        assert_missing_references_are_rejected(&mut conn, &ids).await
    })
}
