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
    apply_migrations(&mut conn, "").await?;
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
    for expected in [
        "idx_user_permissions_perm",
        "idx_user_permissions_user",
        "sqlite_autoindex_user_permissions_1",
    ] {
        assert!(user_permission_indices.iter().any(|name| name == expected));
    }
    Ok(())
}

async fn assert_sqlite_news_schema(conn: &mut DbConnection) -> TestResult<()> {
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
    apply_migrations(&mut conn, "").await?;

    assert_upgrade_backfills(&mut conn).await?;
    assert_sqlite_aligned_schema(&mut conn).await
}
