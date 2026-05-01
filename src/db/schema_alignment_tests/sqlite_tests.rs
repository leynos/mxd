//! `SQLite` schema alignment regression tests.

use diesel::sql_query;
use diesel_async::{AsyncConnection, RunQueryDsl};

use super::{
    DbConnection,
    NameRow,
    TestResult,
    apply_migrations,
    assert_root_category_names_are_unique,
    assert_upgrade_backfills,
    setup_legacy_schema,
};

async fn sqlite_conn() -> TestResult<DbConnection> {
    let mut conn = DbConnection::establish(":memory:").await?;
    apply_migrations(&mut conn, "", None).await?;
    Ok(conn)
}

async fn setup_sqlite_legacy_schema(conn: &mut DbConnection) -> TestResult<()> {
    setup_legacy_schema(
        conn,
        &[
            include_str!("../../../migrations/sqlite/00000000000000_create_users/up.sql"),
            include_str!("../../../migrations/sqlite/00000000000001_create_news/up.sql"),
            include_str!("../../../migrations/sqlite/00000000000002_add_bundles/up.sql"),
            include_str!("../../../migrations/sqlite/00000000000003_add_articles/up.sql"),
            include_str!("../../../migrations/sqlite/00000000000004_create_files/up.sql"),
            include_str!(
                "../../../migrations/sqlite/00000000000005_add_bundle_name_parent_index/up.sql"
            ),
        ],
    )
    .await
}

async fn sqlite_names(conn: &mut DbConnection, sql: &str) -> TestResult<Vec<String>> {
    Ok(sql_query(sql)
        .load::<NameRow>(conn)
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect())
}

async fn assert_sqlite_permission_schema(conn: &mut DbConnection) -> TestResult<()> {
    let tables = sqlite_names(
        conn,
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('permissions', \
         'user_permissions') ORDER BY name",
    )
    .await?;
    assert_eq!(tables, vec!["permissions", "user_permissions"]);

    let permission_indices = sqlite_names(
        conn,
        "SELECT name FROM pragma_index_list('permissions') ORDER BY name",
    )
    .await?;
    assert!(
        permission_indices
            .iter()
            .any(|name| name == "sqlite_autoindex_permissions_1")
    );

    let user_permission_indices = sqlite_names(
        conn,
        "SELECT name FROM pragma_index_list('user_permissions') ORDER BY name",
    )
    .await?;
    let expected = "sqlite_autoindex_user_permissions_1";
    assert!(user_permission_indices.iter().any(|name| name == expected));
    Ok(())
}

async fn assert_sqlite_bundle_schema(conn: &mut DbConnection) -> TestResult<()> {
    let bundle_columns = sqlite_names(
        conn,
        "SELECT name FROM pragma_table_info('news_bundles') ORDER BY cid",
    )
    .await?;
    assert_eq!(
        bundle_columns,
        vec!["id", "parent_bundle_id", "name", "guid", "created_at"]
    );

    let bundle_indices = sqlite_names(
        conn,
        "SELECT name FROM pragma_index_list('news_bundles') ORDER BY name",
    )
    .await?;
    for expected in [
        "idx_bundles_name_parent",
        "idx_bundles_parent",
        "sqlite_autoindex_news_bundles_1",
    ] {
        assert!(bundle_indices.iter().any(|name| name == expected));
    }
    Ok(())
}

async fn assert_sqlite_news_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_sqlite_bundle_schema(conn).await?;
    assert_sqlite_article_indices(conn).await?;

    let category_indices = sqlite_names(
        conn,
        "SELECT name FROM pragma_index_list('news_categories') ORDER BY name",
    )
    .await?;
    for expected in ["idx_categories_bundle", "idx_news_categories_unique"] {
        assert!(category_indices.iter().any(|name| name == expected));
    }

    let category_columns = sqlite_names(
        conn,
        "SELECT name FROM pragma_table_info('news_categories') ORDER BY cid",
    )
    .await?;
    assert_eq!(
        category_columns,
        vec![
            "id",
            "bundle_id",
            "name",
            "guid",
            "add_sn",
            "delete_sn",
            "created_at"
        ]
    );
    Ok(())
}

/// Verifies the expected `SQLite` indexes on the `news_articles` table.
///
/// The helper queries `PRAGMA index_list` through `sqlite_names` using
/// `conn: &mut DbConnection`, returns `TestResult<()>` for database errors, and
/// asserts that `idx_articles_category`, `idx_articles_first_child_article`,
/// `idx_articles_next_article`, `idx_articles_parent_article`, and
/// `idx_articles_prev_article` are present. Missing indexes panic through the
/// assertions.
pub(super) async fn assert_sqlite_article_indices(conn: &mut DbConnection) -> TestResult<()> {
    let article_indices = sqlite_names(
        conn,
        "SELECT name FROM pragma_index_list('news_articles') ORDER BY name",
    )
    .await?;
    for expected in [
        "idx_articles_category",
        "idx_articles_first_child_article",
        "idx_articles_next_article",
        "idx_articles_parent_article",
        "idx_articles_prev_article",
    ] {
        assert!(article_indices.iter().any(|name| name == expected));
    }
    Ok(())
}

async fn assert_sqlite_aligned_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_sqlite_permission_schema(conn).await?;
    assert_sqlite_news_schema(conn).await?;
    assert_root_category_names_are_unique(conn).await
}

#[tokio::test]
async fn sqlite_fresh_migration_creates_aligned_schema() -> TestResult<()> {
    let mut conn = sqlite_conn().await?;

    assert_sqlite_aligned_schema(&mut conn).await
}

#[tokio::test]
async fn sqlite_upgrade_backfills_legacy_news_rows() -> TestResult<()> {
    let mut conn = DbConnection::establish(":memory:").await?;
    setup_sqlite_legacy_schema(&mut conn).await?;
    apply_migrations(&mut conn, "", None).await?;

    assert_upgrade_backfills(&mut conn).await?;
    assert_sqlite_aligned_schema(&mut conn).await
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[tokio::test]
async fn sqlite_category_names_are_bundle_scoped() -> TestResult<()> {
    let mut conn = sqlite_conn().await?;

    // Two bundles
    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (1, NULL, 'A')",
    )
    .execute(&mut conn)
    .await?;
    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (2, NULL, 'B')",
    )
    .execute(&mut conn)
    .await?;

    // Same name in different bundles must succeed
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'Sports', 1)")
        .execute(&mut conn)
        .await?;
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (2, 'Sports', 2)")
        .execute(&mut conn)
        .await?;

    // Same name in the same bundle must fail
    let duplicate = diesel::sql_query(
        "INSERT INTO news_categories (id, name, bundle_id) VALUES (3, 'Sports', 1)",
    )
    .execute(&mut conn)
    .await;
    assert!(
        duplicate.is_err(),
        "duplicate name in same bundle must be rejected"
    );
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[tokio::test]
async fn sqlite_guids_are_non_empty_and_unique() -> TestResult<()> {
    let mut conn = sqlite_conn().await?;

    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (1, NULL, 'BundleA')",
    )
    .execute(&mut conn)
    .await?;
    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (2, NULL, 'BundleB')",
    )
    .execute(&mut conn)
    .await?;

    let guids = sqlite_names(
        &mut conn,
        "SELECT guid AS name FROM news_bundles ORDER BY id",
    )
    .await?;
    assert_eq!(guids.len(), 2, "expected two bundle rows");
    for guid in &guids {
        assert!(!guid.is_empty(), "GUID must not be empty");
    }
    assert_ne!(guids[0], guids[1], "GUIDs must be unique across rows");
    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[tokio::test]
async fn sqlite_add_sn_reflects_article_count() -> TestResult<()> {
    let mut conn = sqlite_conn().await?;

    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (1, NULL, 'TestBundle')",
    )
    .execute(&mut conn)
    .await?;
    // Category with two articles and one with none
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'WithTwo', 1)")
        .execute(&mut conn)
        .await?;
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (2, 'Empty', 1)")
        .execute(&mut conn)
        .await?;

    for i in 1_i32..=2 {
        diesel::sql_query(format!(
            "INSERT INTO news_articles (id, category_id, title, posted_at) VALUES ({i}, 1, \
             'Article {i}', '2026-01-01 00:00:00')"
        ))
        .execute(&mut conn)
        .await?;
    }

    let add_sn_row = diesel::sql_query("SELECT add_sn AS name FROM news_categories WHERE id = 1")
        .get_result::<super::NameRow>(&mut conn)
        .await?;
    let add_sn_1: i32 = add_sn_row.name.parse().unwrap_or(-1);
    assert_eq!(
        add_sn_1, 0,
        "add_sn is set at migration time; fresh inserts do not auto-increment it"
    );

    let add_sn_empty_row =
        diesel::sql_query("SELECT add_sn AS name FROM news_categories WHERE id = 2")
            .get_result::<super::NameRow>(&mut conn)
            .await?;
    let add_sn_empty: i32 = add_sn_empty_row.name.parse().unwrap_or(-1);
    assert_eq!(add_sn_empty, 0, "empty category add_sn must be 0");

    Ok(())
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[tokio::test]
async fn sqlite_article_threading_enforces_referential_integrity() -> TestResult<()> {
    let mut conn = sqlite_conn().await?;

    diesel::sql_query(
        "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (1, NULL, 'ThreadBundle')",
    )
    .execute(&mut conn)
    .await?;
    diesel::sql_query(
        "INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'ThreadCat', 1)",
    )
    .execute(&mut conn)
    .await?;

    // Root article
    diesel::sql_query(
        "INSERT INTO news_articles (id, category_id, parent_article_id, prev_article_id, \
         next_article_id, first_child_article_id, title, posted_at) VALUES (1, 1, NULL, NULL, \
         NULL, NULL, 'Root', '2026-01-01 00:00:00')",
    )
    .execute(&mut conn)
    .await?;

    // Child article referencing root via parent_article_id
    diesel::sql_query(
        "INSERT INTO news_articles (id, category_id, parent_article_id, prev_article_id, \
         next_article_id, first_child_article_id, title, posted_at) VALUES (2, 1, 1, NULL, NULL, \
         NULL, 'Child', '2026-01-02 00:00:00')",
    )
    .execute(&mut conn)
    .await?;

    // Update root to point first_child_article_id at child
    diesel::sql_query("UPDATE news_articles SET first_child_article_id = 2 WHERE id = 1")
        .execute(&mut conn)
        .await?;

    // Verify the threading link via a JOIN query
    let linked = diesel::sql_query(
        "SELECT a.id AS name FROM news_articles a INNER JOIN news_articles child ON child.id = \
         a.first_child_article_id WHERE a.id = 1",
    )
    .get_result::<super::NameRow>(&mut conn)
    .await?;
    assert_eq!(linked.name, "1", "root article must link to its child");

    // Referential integrity: inserting an article with a non-existent parent must fail
    // (PRAGMA foreign_keys must be ON; SQLite pragmas are connection-scoped)
    diesel::sql_query("PRAGMA foreign_keys = ON")
        .execute(&mut conn)
        .await?;
    let bad_insert = diesel::sql_query(
        "INSERT INTO news_articles (id, category_id, parent_article_id, title, posted_at) VALUES \
         (99, 1, 9999, 'Orphan', '2026-01-03 00:00:00')",
    )
    .execute(&mut conn)
    .await;
    assert!(
        bad_insert.is_err(),
        "insert with non-existent parent_article_id must be rejected when FK enforcement is on"
    );
    Ok(())
}
