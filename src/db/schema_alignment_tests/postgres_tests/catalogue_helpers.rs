//! Shared `PostgreSQL` helpers for schema-alignment tests.
//!
//! Provides:
//! - `setup_postgres_legacy_schema` to stand up legacy DDL prior to an upgrade.
//! - `postgres_names` to read catalogue names (tables/columns/indices) as strings.
//! - `postgres_aligned_schema_is_valid` to check the final schema surface.
//! - `postgres_article_indices_are_present` to validate threading-related indices.
//! - `permission_round_trip_is_seeded_and_valid` to run a permission join smoke-test using fixed
//!   IDs.
//!
//! Relationship to parent:
//! - Called by `postgres_tests` module tests; composes with `assert_upgrade_backfills` from
//!   `schema_alignment_tests::mod`.

use std::future::Future;

use diesel::sql_query;
use diesel_async::RunQueryDsl;
use test_util::postgres::PostgresTestDb;

use super::super::{
    DbConnection,
    NameRow,
    PermissionTestIds,
    TestResult,
    permission_round_trip_is_valid,
    seed_permission_round_trip,
    setup_legacy_schema,
    verify_root_category_names_are_unique_with_constraint_insert,
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
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect())
}

/// Returns `true` when both `permissions` and `user_permissions` exist with
/// their expected indices in the public schema.
async fn postgres_permission_schema_is_aligned(conn: &mut DbConnection) -> TestResult<bool> {
    let permission_tables = postgres_names(
        conn,
        "SELECT table_name AS name FROM information_schema.tables WHERE table_schema = 'public' \
         AND table_name IN ('permissions', 'user_permissions') ORDER BY table_name",
    )
    .await?;
    if permission_tables != vec!["permissions", "user_permissions"] {
        return Ok(false);
    }

    let permission_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE schemaname = 'public' AND tablename IN \
         ('permissions', 'user_permissions') ORDER BY indexname",
    )
    .await?;
    Ok(["permissions_code_key", "user_permissions_pkey"]
        .iter()
        .all(|expected| permission_indices.iter().any(|name| name == expected)))
}

/// Returns `true` when `news_bundles` has the expected columns and indices in
/// the public schema.
async fn postgres_bundle_schema_is_aligned(conn: &mut DbConnection) -> TestResult<bool> {
    let bundle_columns = postgres_names(
        conn,
        "SELECT column_name AS name FROM information_schema.columns WHERE table_name = \
         'news_bundles' AND table_schema = 'public' ORDER BY ordinal_position",
    )
    .await?;
    if bundle_columns != vec!["id", "parent_bundle_id", "name", "guid", "created_at"] {
        return Ok(false);
    }

    let bundle_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE tablename = 'news_bundles' AND schemaname \
         = 'public' ORDER BY indexname",
    )
    .await?;
    if ![
        "idx_bundles_name_parent",
        "idx_bundles_parent",
        "news_bundles_name_parent_bundle_id_key",
    ]
    .iter()
    .all(|expected| bundle_indices.iter().any(|name| name == expected))
    {
        return Ok(false);
    }

    let bundle_constraints = postgres_names(
        conn,
        "SELECT conname AS name FROM pg_constraint WHERE conrelid = \
         'public.news_bundles'::regclass AND contype = 'u' ORDER BY conname",
    )
    .await?;
    Ok(bundle_constraints
        .iter()
        .any(|name| name == "news_bundles_name_parent_bundle_id_key"))
}

/// Returns `true` when `news_categories` has the expected columns and indices
/// in the public schema.
async fn postgres_category_schema_is_aligned(conn: &mut DbConnection) -> TestResult<bool> {
    let category_columns = postgres_names(
        conn,
        "SELECT column_name AS name FROM information_schema.columns WHERE table_name = \
         'news_categories' AND table_schema = 'public' ORDER BY ordinal_position",
    )
    .await?;
    if category_columns
        != vec![
            "id",
            "name",
            "bundle_id",
            "guid",
            "add_sn",
            "delete_sn",
            "created_at",
        ]
    {
        return Ok(false);
    }

    let category_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE tablename = 'news_categories' AND \
         schemaname = 'public' ORDER BY indexname",
    )
    .await?;
    Ok([
        "idx_categories_bundle",
        "idx_categories_root_name_unique",
        "idx_categories_name_bundle_unique",
    ]
    .iter()
    .all(|expected| category_indices.iter().any(|name| name == expected)))
}

/// Returns `true` when `news_articles` carries all expected threading indices.
///
/// The predicate queries `pg_indexes` through `postgres_names` and checks that
/// `idx_articles_category`, `idx_articles_first_child_article`,
/// `idx_articles_next_article`, `idx_articles_parent_article`, and
/// `idx_articles_prev_article` are all present. Database query failures are
/// propagated as `Err`; a missing index causes the predicate to return `false`.
pub(crate) async fn postgres_article_indices_are_present(
    conn: &mut DbConnection,
) -> TestResult<bool> {
    let article_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE tablename = 'news_articles' AND \
         schemaname = 'public' ORDER BY indexname",
    )
    .await?;
    Ok([
        "idx_articles_category",
        "idx_articles_first_child_article",
        "idx_articles_next_article",
        "idx_articles_parent_article",
        "idx_articles_prev_article",
    ]
    .iter()
    .all(|expected| article_indices.iter().any(|name| name == expected)))
}

/// Returns `true` when the full `news_bundles`, `news_categories`, and
/// `news_articles` schema is aligned, including columns and indices.
async fn postgres_news_schema_is_aligned(conn: &mut DbConnection) -> TestResult<bool> {
    Ok(postgres_bundle_schema_is_aligned(conn).await?
        && postgres_category_schema_is_aligned(conn).await?
        && postgres_article_indices_are_present(conn).await?)
}

/// Returns `true` when the full aligned schema is present across permission
/// tables, news tables, and the root-category uniqueness constraint.
///
/// Note: this predicate also runs a constraint-verification insert to confirm
/// the partial unique index on root categories. This insert is a side-effect
/// required by the constraint check and is isolated in
/// `verify_root_category_names_are_unique_with_constraint_insert`.
pub(super) async fn postgres_aligned_schema_is_valid(conn: &mut DbConnection) -> TestResult<bool> {
    if !postgres_permission_schema_is_aligned(conn).await? {
        return Ok(false);
    }
    if !postgres_news_schema_is_aligned(conn).await? {
        return Ok(false);
    }
    verify_root_category_names_are_unique_with_constraint_insert(conn).await?;
    Ok(true)
}

/// Seeds a permission round-trip using fixed IDs and returns `true` when the
/// join query confirms exactly one matching row.
pub(super) async fn permission_round_trip_is_seeded_and_valid(
    conn: &mut DbConnection,
) -> TestResult<bool> {
    seed_permission_round_trip(
        conn,
        PermissionTestIds {
            user_id: 42,
            permission_id: 42,
            code: 34,
        },
    )
    .await?;
    permission_round_trip_is_valid(
        conn,
        PermissionTestIds {
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
    pg.start().await?;
    Ok(Some(pg))
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
