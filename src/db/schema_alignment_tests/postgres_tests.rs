//! `PostgreSQL` schema alignment regression tests.

use std::future::Future;

use diesel::sql_query;
use diesel_async::{AsyncConnection, RunQueryDsl};
use test_util::postgres::{PostgresTestDb, PostgresTestDbError};

use super::{
    DbConnection,
    NameRow,
    TestResult,
    apply_migrations,
    assert_permission_round_trip_with_ids,
    assert_root_category_names_are_unique,
    assert_upgrade_backfills,
    setup_legacy_schema,
};

async fn setup_postgres_legacy_schema(conn: &mut DbConnection) -> TestResult<()> {
    setup_legacy_schema(
        conn,
        &[
            include_str!("../../../migrations/postgres/00000000000000_create_users/up.sql"),
            include_str!("../../../migrations/postgres/00000000000001_create_news/up.sql"),
            include_str!("../../../migrations/postgres/00000000000002_add_bundles/up.sql"),
            include_str!("../../../migrations/postgres/00000000000003_add_articles/up.sql"),
            include_str!("../../../migrations/postgres/00000000000004_create_files/up.sql"),
            include_str!(
                "../../../migrations/postgres/00000000000005_add_bundle_name_parent_index/up.sql"
            ),
        ],
    )
    .await
}

async fn postgres_names(conn: &mut DbConnection, sql: &str) -> TestResult<Vec<String>> {
    Ok(sql_query(sql)
        .load::<NameRow>(conn)
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect())
}

async fn assert_postgres_permission_schema(conn: &mut DbConnection) -> TestResult<()> {
    let permission_tables = postgres_names(
        conn,
        "SELECT table_name AS name FROM information_schema.tables WHERE table_schema = 'public' \
         AND table_name IN ('permissions', 'user_permissions') ORDER BY table_name",
    )
    .await?;
    assert_eq!(permission_tables, vec!["permissions", "user_permissions"]);

    let permission_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE schemaname = 'public' AND tablename IN \
         ('permissions', 'user_permissions') ORDER BY indexname",
    )
    .await?;
    for expected in ["permissions_code_key", "user_permissions_pkey"] {
        assert!(permission_indices.iter().any(|name| name == expected));
    }
    Ok(())
}

async fn assert_postgres_bundle_schema(conn: &mut DbConnection) -> TestResult<()> {
    let bundle_columns = postgres_names(
        conn,
        "SELECT column_name AS name FROM information_schema.columns WHERE table_name = \
         'news_bundles' AND table_schema = 'public' ORDER BY ordinal_position",
    )
    .await?;
    assert_eq!(
        bundle_columns,
        vec!["id", "parent_bundle_id", "name", "guid", "created_at"]
    );

    let bundle_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE tablename = 'news_bundles' AND schemaname \
         = 'public' ORDER BY indexname",
    )
    .await?;
    for expected in [
        "idx_bundles_name_parent",
        "idx_bundles_parent",
        "news_bundles_name_parent_bundle_id_key",
    ] {
        assert!(bundle_indices.iter().any(|name| name == expected));
    }

    let bundle_constraints = postgres_names(
        conn,
        "SELECT conname AS name FROM pg_constraint WHERE conrelid = \
         'public.news_bundles'::regclass AND contype = 'u' ORDER BY conname",
    )
    .await?;
    assert!(
        bundle_constraints
            .iter()
            .any(|name| name == "news_bundles_name_parent_bundle_id_key")
    );
    Ok(())
}

async fn assert_postgres_category_schema(conn: &mut DbConnection) -> TestResult<()> {
    let category_columns = postgres_names(
        conn,
        "SELECT column_name AS name FROM information_schema.columns WHERE table_name = \
         'news_categories' AND table_schema = 'public' ORDER BY ordinal_position",
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

    let category_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE tablename = 'news_categories' AND \
         schemaname = 'public' ORDER BY indexname",
    )
    .await?;
    for expected in [
        "idx_categories_bundle",
        "idx_categories_root_name_unique",
        "idx_categories_name_bundle_unique",
    ] {
        assert!(category_indices.iter().any(|name| name == expected));
    }
    Ok(())
}

/// Verifies that `news_articles` has the expected `PostgreSQL` indexes.
///
/// The check is order-agnostic: it queries index names with `postgres_names`
/// through the supplied `DbConnection` and asserts that `idx_articles_category`,
/// `idx_articles_first_child_article`, `idx_articles_next_article`,
/// `idx_articles_parent_article`, and `idx_articles_prev_article` are present.
/// Database query failures are returned as `TestResult<()>`; missing indexes
/// panic through the assertions.
pub(super) async fn assert_postgres_article_indices(conn: &mut DbConnection) -> TestResult<()> {
    let article_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE tablename = 'news_articles' AND \
         schemaname = 'public' ORDER BY indexname",
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

async fn assert_postgres_news_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_postgres_bundle_schema(conn).await?;
    assert_postgres_category_schema(conn).await?;
    assert_postgres_article_indices(conn).await
}

async fn assert_postgres_aligned_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_postgres_permission_schema(conn).await?;
    assert_postgres_news_schema(conn).await?;
    assert_root_category_names_are_unique(conn).await
}

async fn assert_permission_round_trip(conn: &mut DbConnection) -> TestResult<()> {
    assert_permission_round_trip_with_ids(conn, 42, 42, 34).await
}

fn start_embedded_postgres_db() -> TestResult<Option<PostgresTestDb>> {
    match PostgresTestDb::new() {
        Ok(db) => Ok(Some(db)),
        Err(PostgresTestDbError::Unavailable(_)) => {
            tracing::warn!("SKIP-TEST-CLUSTER: PostgreSQL unavailable");
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

fn with_postgres_test_db<F, Fut>(test: F) -> TestResult<()>
where
    F: FnOnce(String) -> Fut + Send + 'static,
    Fut: Future<Output = TestResult<()>> + Send + 'static,
{
    let Some(db) = embedded_postgres_db()? else {
        return Ok(());
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move { test(db.url.to_string()).await })
}

#[test]
fn postgres_fresh_migration_creates_aligned_schema() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        assert_postgres_aligned_schema(&mut conn).await?;
        assert_permission_round_trip(&mut conn).await
    })
}

#[test]
fn postgres_upgrade_backfills_legacy_news_rows() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        setup_postgres_legacy_schema(&mut conn).await?;
        apply_migrations(&mut conn, &url, None).await?;

        assert_upgrade_backfills(&mut conn).await?;
        assert_postgres_aligned_schema(&mut conn).await
    })
}

fn embedded_postgres_db() -> TestResult<Option<PostgresTestDb>> { start_embedded_postgres_db() }
