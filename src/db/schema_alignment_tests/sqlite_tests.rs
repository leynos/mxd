//! `SQLite` schema-alignment regression tests for roadmap item 4.1.1.
//!
//! Scope:
//! - Validates aligned schema on fresh migration and on upgrade from legacy rebuilds.
//! - Asserts bundle schema columns, category partial uniqueness at root, GUID non-emptiness and
//!   uniqueness, threading referential integrity, and article-index presence.
//!
//! Helpers:
//! - Uses in-memory connections, `sqlite_names` catalogue readers, and the parent module's
//!   `assert_upgrade_backfills`.

use anyhow::Context;
use diesel::sql_query;
use diesel_async::{AsyncConnection, RunQueryDsl};
use rstest::{fixture, rstest};

use super::{
    DbConnection,
    NameRow,
    PermissionTestIds,
    TestResult,
    apply_migrations,
    assert_upgrade_backfills,
    seed_permission_round_trip,
    seed_root_category_name_conflict,
    setup_legacy_schema,
    verify_root_category_constraint_error,
};

async fn sqlite_conn() -> TestResult<DbConnection> {
    let mut conn = DbConnection::establish(":memory:").await?;
    apply_migrations(&mut conn, "", None).await?;
    Ok(conn)
}

#[fixture]
async fn two_bundle_db() -> TestResult<DbConnection> {
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
async fn add_sn_db(#[future] two_bundle_db: TestResult<DbConnection>) -> TestResult<DbConnection> {
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
async fn threaded_articles_db() -> TestResult<DbConnection> {
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
        .await
        .with_context(|| format!("LOAD SQLite names: {sql}"))?
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
    anyhow::ensure!(
        tables == vec!["permissions", "user_permissions"],
        "expected SQLite permission tables, got {tables:?}"
    );

    let permission_indices = sqlite_names(
        conn,
        "SELECT name FROM pragma_index_list('permissions') ORDER BY name",
    )
    .await?;
    anyhow::ensure!(
        permission_indices
            .iter()
            .any(|name| name == "sqlite_autoindex_permissions_1"),
        "missing SQLite permissions unique index"
    );

    let user_permission_indices = sqlite_names(
        conn,
        "SELECT name FROM pragma_index_list('user_permissions') ORDER BY name",
    )
    .await?;
    let expected = "sqlite_autoindex_user_permissions_1";
    anyhow::ensure!(
        user_permission_indices.iter().any(|name| name == expected),
        "missing SQLite user_permissions index {expected}"
    );
    Ok(())
}

async fn assert_sqlite_bundle_schema(conn: &mut DbConnection) -> TestResult<()> {
    let bundle_columns = sqlite_names(
        conn,
        "SELECT name FROM pragma_table_info('news_bundles') ORDER BY cid",
    )
    .await?;
    anyhow::ensure!(
        bundle_columns == vec!["id", "parent_bundle_id", "name", "guid", "created_at"],
        "unexpected SQLite news_bundles columns: {bundle_columns:?}"
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
        anyhow::ensure!(
            bundle_indices.iter().any(|name| name == expected),
            "missing SQLite bundle index {expected}"
        );
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
        anyhow::ensure!(
            category_indices.iter().any(|name| name == expected),
            "missing SQLite category index {expected}"
        );
    }

    let category_columns = sqlite_names(
        conn,
        "SELECT name FROM pragma_table_info('news_categories') ORDER BY cid",
    )
    .await?;
    anyhow::ensure!(
        category_columns
            == vec![
                "id",
                "bundle_id",
                "name",
                "guid",
                "add_sn",
                "delete_sn",
                "created_at"
            ],
        "unexpected SQLite news_categories columns: {category_columns:?}"
    );
    Ok(())
}

/// Verifies the expected `SQLite` indexes on the `news_articles` table.
///
/// The helper queries `PRAGMA index_list` through `sqlite_names` using
/// `conn: &mut DbConnection`, returns `TestResult<()>` for database errors, and
/// asserts that `idx_articles_category`, `idx_articles_first_child_article`,
/// `idx_articles_next_article`, `idx_articles_parent_article`, and
/// `idx_articles_prev_article` are present. Missing indexes return
/// `TestResult` errors.
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
        anyhow::ensure!(
            article_indices.iter().any(|name| name == expected),
            "missing SQLite article index {expected}"
        );
    }
    Ok(())
}

async fn assert_sqlite_aligned_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_sqlite_permission_schema(conn).await?;
    assert_sqlite_news_schema(conn).await?;
    let conflict_result = seed_root_category_name_conflict(conn).await;
    verify_root_category_constraint_error(conflict_result).await
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

    seed_permission_round_trip(
        &mut conn,
        PermissionTestIds {
            user_id: 84,
            permission_id: 84,
            code: 84,
        },
    )
    .await?;
    assert_upgrade_backfills(&mut conn).await?;
    assert_sqlite_aligned_schema(&mut conn).await
}

#[rstest]
#[tokio::test]
async fn sqlite_category_names_are_bundle_scoped(
    #[future] two_bundle_db: TestResult<DbConnection>,
) -> TestResult<()> {
    let mut conn = two_bundle_db.await?;

    // Same name in different bundles must succeed
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'Sports', 1)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert first SQLite scoped Sports category")?;
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (2, 'Sports', 2)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert second SQLite scoped Sports category")?;

    // Same name in the same bundle must fail
    let duplicate = diesel::sql_query(
        "INSERT INTO news_categories (id, name, bundle_id) VALUES (3, 'Sports', 1)",
    )
    .execute(&mut conn)
    .await;
    anyhow::ensure!(
        duplicate.is_err(),
        "duplicate name in same bundle must be rejected"
    );
    Ok(())
}

#[rstest]
#[tokio::test]
async fn sqlite_guids_are_non_empty_and_unique(
    #[future] two_bundle_db: TestResult<DbConnection>,
) -> TestResult<()> {
    let mut conn = two_bundle_db.await?;

    let guids = sqlite_names(
        &mut conn,
        "SELECT guid AS name FROM news_bundles ORDER BY id",
    )
    .await?;
    anyhow::ensure!(guids.len() == 2, "expected two bundle rows");
    for guid in &guids {
        anyhow::ensure!(!guid.is_empty(), "GUID must not be empty");
    }
    let guid_set: std::collections::HashSet<_> = guids.iter().collect();
    anyhow::ensure!(
        guid_set.len() == guids.len(),
        "GUIDs must be unique across rows"
    );

    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'CA', 1)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert first SQLite GUID category")?;
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (2, 'CB', 2)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert second SQLite GUID category")?;

    let category_guids = sqlite_names(
        &mut conn,
        "SELECT guid AS name FROM news_categories ORDER BY id",
    )
    .await?;
    anyhow::ensure!(category_guids.len() == 2, "expected two category rows");
    for guid in &category_guids {
        anyhow::ensure!(!guid.is_empty(), "category GUID must not be empty");
    }
    let category_guid_set: std::collections::HashSet<_> = category_guids.iter().collect();
    anyhow::ensure!(
        category_guid_set.len() == category_guids.len(),
        "category GUIDs must be unique"
    );
    Ok(())
}

#[rstest]
#[tokio::test]
async fn sqlite_add_sn_defaults_to_zero_for_fresh_inserts(
    #[future] add_sn_db: TestResult<DbConnection>,
) -> TestResult<()> {
    let mut conn = add_sn_db.await?;

    let add_sn_row = diesel::sql_query("SELECT add_sn AS name FROM news_categories WHERE id = 1")
        .get_result::<super::NameRow>(&mut conn)
        .await
        .context("LOAD SQLite add_sn for category id=1")?;
    let add_sn_1: i32 = add_sn_row
        .name
        .parse()
        .context("failed to parse add_sn for id=1")?;
    anyhow::ensure!(
        add_sn_1 == 0,
        "add_sn is set at migration time; fresh inserts do not auto-increment it"
    );

    let add_sn_empty_row =
        diesel::sql_query("SELECT add_sn AS name FROM news_categories WHERE id = 2")
            .get_result::<super::NameRow>(&mut conn)
            .await
            .context("LOAD SQLite add_sn for category id=2")?;
    let add_sn_empty: i32 = add_sn_empty_row
        .name
        .parse()
        .context("failed to parse add_sn for id=2")?;
    anyhow::ensure!(add_sn_empty == 0, "empty category add_sn must be 0");

    Ok(())
}

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
    .get_result::<super::NameRow>(&mut conn)
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
    anyhow::ensure!(
        bad_insert.is_err(),
        "insert with non-existent parent_article_id must be rejected when FK enforcement is on"
    );
    let bad_update =
        diesel::sql_query("UPDATE news_articles SET first_child_article_id = 9999 WHERE id = 1")
            .execute(&mut conn)
            .await;
    anyhow::ensure!(
        bad_update.is_err(),
        "update with non-existent first_child_article_id must be rejected when FK enforcement is \
         on"
    );
    Ok(())
}
