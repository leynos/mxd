//! Shared `PostgreSQL` helpers for schema-alignment tests.
//!
//! Provides:
//! - `setup_postgres_legacy_schema` to stand up legacy DDL prior to an upgrade.
//! - `postgres_names` to read catalogue names (tables/columns/indices) as strings.
//! - `assert_postgres_aligned_schema` to assert the final schema surface.
//! - `assert_postgres_article_indices` to validate threading-related indices.
//! - `assert_permission_round_trip` to run a permission join smoke-test using fixed IDs.
//!
//! Relationship to parent:
//! - Called by `postgres_tests` module tests; composes with the backfill seed/assertion helpers
//!   from `schema_alignment_tests::mod`.

use std::future::Future;

use anyhow::Context;
use diesel::sql_query;
use diesel_async::RunQueryDsl;
use test_util::postgres::PostgresTestDb;

use super::super::{
    DbConnection,
    NameRow,
    TestResult,
    assert_permission_round_trip_with_ids,
    seed_permission_round_trip,
    seed_root_category_name_conflict,
    setup_legacy_schema,
    verify_root_category_constraint_error,
};

pub(super) async fn setup_postgres_legacy_schema(conn: &mut DbConnection) -> TestResult<()> {
    setup_legacy_schema(
        conn,
        &[
            include_str!("../../../../migrations/postgres/00000000000000_create_users/up.sql"),
            include_str!("../../../../migrations/postgres/00000000000001_create_news/up.sql"),
            include_str!("../../../../migrations/postgres/00000000000002_add_bundles/up.sql"),
            include_str!("../../../../migrations/postgres/00000000000003_add_articles/up.sql"),
            include_str!("../../../../migrations/postgres/00000000000004_create_files/up.sql"),
            include_str!(
                "../../../../migrations/postgres/00000000000005_add_bundle_name_parent_index/up.\
                 sql"
            ),
        ],
    )
    .await
}

pub(super) async fn postgres_names(conn: &mut DbConnection, sql: &str) -> TestResult<Vec<String>> {
    Ok(sql_query(sql)
        .load::<NameRow>(conn)
        .await
        .with_context(|| format!("LOAD PostgreSQL names: {sql}"))?
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
    anyhow::ensure!(
        permission_tables == vec!["permissions", "user_permissions"],
        "expected permissions tables, got {permission_tables:?}"
    );

    let permission_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE schemaname = 'public' AND tablename IN \
         ('permissions', 'user_permissions') ORDER BY indexname",
    )
    .await?;
    for expected in [
        "permissions_code_key",
        "permissions_pkey",
        "user_permissions_permission_id_idx",
        "user_permissions_pkey",
        "user_permissions_user_id_idx",
    ] {
        anyhow::ensure!(
            permission_indices.iter().any(|name| name == expected),
            "missing PostgreSQL permission index {expected}; got {permission_indices:?}"
        );
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
    anyhow::ensure!(
        bundle_columns == vec!["id", "parent_bundle_id", "name", "guid", "created_at"],
        "unexpected news_bundles columns: {bundle_columns:?}"
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
        "idx_bundles_root_name_unique",
        "news_bundles_name_parent_bundle_id_key",
    ] {
        anyhow::ensure!(
            bundle_indices.iter().any(|name| name == expected),
            "missing PostgreSQL bundle index {expected}"
        );
    }

    let bundle_constraints = postgres_names(
        conn,
        "SELECT conname AS name FROM pg_constraint WHERE conrelid = \
         'public.news_bundles'::regclass AND contype = 'u' ORDER BY conname",
    )
    .await?;
    anyhow::ensure!(
        bundle_constraints
            .iter()
            .any(|name| name == "news_bundles_name_parent_bundle_id_key"),
        "missing PostgreSQL bundle unique constraint"
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
    anyhow::ensure!(
        category_columns
            == vec![
                "id",
                "name",
                "bundle_id",
                "guid",
                "add_sn",
                "delete_sn",
                "created_at"
            ],
        "unexpected news_categories columns: {category_columns:?}"
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
        anyhow::ensure!(
            category_indices.iter().any(|name| name == expected),
            "missing PostgreSQL category index {expected}"
        );
    }
    Ok(())
}

async fn assert_postgres_article_schema(conn: &mut DbConnection) -> TestResult<()> {
    let article_columns = postgres_names(
        conn,
        "SELECT column_name AS name FROM information_schema.columns WHERE table_name = \
         'news_articles' AND table_schema = 'public' ORDER BY ordinal_position",
    )
    .await?;
    anyhow::ensure!(
        article_columns
            == vec![
                "id",
                "category_id",
                "parent_article_id",
                "prev_article_id",
                "next_article_id",
                "first_child_article_id",
                "title",
                "poster",
                "posted_at",
                "flags",
                "data_flavor",
                "data"
            ],
        "unexpected news_articles columns: {article_columns:?}"
    );
    Ok(())
}

/// Verifies that `news_articles` has the expected `PostgreSQL` indexes.
///
/// The check is order-agnostic: it queries index names with `postgres_names`
/// through the supplied `DbConnection` and asserts that `idx_articles_category`,
/// `idx_articles_first_child_article`, `idx_articles_next_article`,
/// `idx_articles_parent_article`, and `idx_articles_prev_article` are present.
/// Database query failures are returned as `TestResult<()>`; missing indexes
/// return `TestResult` errors.
pub(crate) async fn assert_postgres_article_indices(conn: &mut DbConnection) -> TestResult<()> {
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
        anyhow::ensure!(
            article_indices.iter().any(|name| name == expected),
            "missing PostgreSQL article index {expected}"
        );
    }
    Ok(())
}

async fn assert_postgres_news_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_postgres_bundle_schema(conn).await?;
    assert_postgres_category_schema(conn).await?;
    assert_postgres_article_schema(conn).await?;
    assert_postgres_article_indices(conn).await
}

pub(super) async fn assert_postgres_aligned_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_postgres_permission_schema(conn).await?;
    assert_postgres_news_schema(conn).await?;
    let conflict_result = seed_root_category_name_conflict(conn).await;
    verify_root_category_constraint_error(conflict_result).await
}

pub(super) async fn assert_permission_round_trip(conn: &mut DbConnection) -> TestResult<()> {
    seed_permission_round_trip(
        conn,
        super::super::PermissionTestIds {
            user_id: 42,
            permission_id: 42,
            code: 34,
        },
    )
    .await?;
    assert_permission_round_trip_with_ids(
        conn,
        super::super::PermissionTestIds {
            user_id: 42,
            permission_id: 42,
            code: 34,
        },
    )
    .await
}

fn is_ci() -> bool { std::env::var("CI").is_ok_and(|value| !value.is_empty()) }

pub(super) fn with_postgres_test_db<F, Fut>(test: F) -> TestResult<()>
where
    F: FnOnce(String) -> Fut + Send + 'static,
    Fut: Future<Output = TestResult<()>> + Send + 'static,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        if std::env::var_os("POSTGRES_TEST_URL").is_some() {
            let db = PostgresTestDb::new_async().await?;
            return test(db.url.to_string()).await;
        }

        run_with_embedded_postgres(test).await
    })
}

async fn run_with_embedded_postgres<F, Fut>(test: F) -> TestResult<()>
where
    F: FnOnce(String) -> Fut + Send + 'static,
    Fut: Future<Output = TestResult<()>> + Send + 'static,
{
    let Some(pg) = start_optional_embedded_postgres().await? else {
        return Ok(());
    };

    let db_name = format!(
        "schema_alignment_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let result = async {
        pg.create_database(&db_name).await?;
        let url = pg.settings().url(&db_name);
        test(url).await
    }
    .await;

    let stop_result = stop_embedded_postgres(pg);
    combine_postgres_test_result(result, stop_result)
}

async fn start_optional_embedded_postgres() -> TestResult<Option<postgresql_embedded::PostgreSQL>> {
    let mut pg = postgresql_embedded::PostgreSQL::default();
    if let Err(error) = pg.setup().await {
        if is_ci() {
            return Err(error.into());
        }
        tracing::warn!("SKIP-TEST-CLUSTER: PostgreSQL unavailable");
        return Ok(None);
    }
    handle_optional_postgres_start(pg).await
}

async fn handle_optional_postgres_start(
    mut pg: postgresql_embedded::PostgreSQL,
) -> TestResult<Option<postgresql_embedded::PostgreSQL>> {
    let Err(error) = pg.start().await else {
        return Ok(Some(pg));
    };
    handle_optional_postgres_error(error)
}

fn handle_optional_postgres_error(
    error: impl Into<anyhow::Error>,
) -> TestResult<Option<postgresql_embedded::PostgreSQL>> {
    if is_ci() {
        return Err(error.into());
    }
    tracing::warn!("SKIP-TEST-CLUSTER: PostgreSQL unavailable");
    Ok(None)
}

fn stop_embedded_postgres(pg: postgresql_embedded::PostgreSQL) -> TestResult<()> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(pg.stop()).map_err(anyhow::Error::from)
    })
    .join()
    .map_err(|_| anyhow::anyhow!("embedded postgres shutdown thread panicked"))?
}

fn combine_postgres_test_result(
    result: TestResult<()>,
    stop_result: TestResult<()>,
) -> TestResult<()> {
    match (result, stop_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(test_error), Ok(())) => Err(test_error),
        (Ok(()), Err(stop_error)) => Err(stop_error),
        (Err(test_error), Err(stop_error)) => Err(anyhow::anyhow!(
            "postgres schema alignment test failed: {test_error}; embedded postgres shutdown \
             failed: {stop_error}"
        )),
    }
}
