//! `SQLite` schema catalogue assertions and root uniqueness behaviour tests.

#![cfg(feature = "sqlite")]

use anyhow::Context;
use diesel_async::RunQueryDsl;
use rstest::rstest;

use super::{
    super::{
        DbConnection,
        TestResult,
        seed_root_category_name_conflict,
        verify_root_category_constraint_error,
    },
    common::{sqlite_names, two_bundle_db},
};

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
pub(in crate::db::schema_alignment_tests) async fn assert_sqlite_article_indices(
    conn: &mut DbConnection,
) -> TestResult<()> {
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

pub(super) async fn assert_sqlite_aligned_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_sqlite_permission_schema(conn).await?;
    assert_sqlite_news_schema(conn).await?;
    let conflict_result = seed_root_category_name_conflict(conn).await;
    verify_root_category_constraint_error(conflict_result).await
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
