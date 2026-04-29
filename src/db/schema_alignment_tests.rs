//! Regression tests for news schema alignment migrations.
//!
//! These tests validate both fresh schema application and upgrades from the
//! pre-4.1.1 news schema so later roadmap work can rely on the aligned
//! persistence contract.

#[cfg(feature = "postgres")]
use std::future::Future;

use chrono::NaiveDateTime;
use diesel::{
    QueryableByName,
    sql_query,
    sql_types::{Integer, Nullable, Text, Timestamp},
};
use diesel_async::{AsyncConnection, RunQueryDsl};
#[cfg(feature = "postgres")]
use test_util::postgres::{PostgresTestDb, PostgresTestDbError};

use super::{DbConnection, apply_migrations};

type TestResult<T> = Result<T, anyhow::Error>;

#[derive(QueryableByName)]
struct NameRow {
    #[diesel(sql_type = Text)]
    name: String,
}

#[cfg(feature = "postgres")]
#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    count: i64,
}

#[derive(QueryableByName)]
struct BundleBackfillRow {
    #[diesel(sql_type = Nullable<Text>)]
    guid: Option<String>,
    #[diesel(sql_type = Nullable<Timestamp>)]
    created_at: Option<NaiveDateTime>,
}

#[derive(QueryableByName)]
struct CategoryBackfillRow {
    #[diesel(sql_type = Nullable<Text>)]
    guid: Option<String>,
    #[diesel(sql_type = Nullable<Integer>)]
    add_sn: Option<i32>,
    #[diesel(sql_type = Nullable<Integer>)]
    delete_sn: Option<i32>,
    #[diesel(sql_type = Nullable<Timestamp>)]
    created_at: Option<NaiveDateTime>,
}

#[cfg(feature = "sqlite")]
async fn sqlite_conn() -> TestResult<DbConnection> {
    let mut conn = DbConnection::establish(":memory:").await?;
    apply_migrations(&mut conn, "").await?;
    Ok(conn)
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn run_statements(conn: &mut DbConnection, statements: &[&str]) -> TestResult<()> {
    for &statement in statements {
        sql_query(statement).execute(conn).await?;
    }
    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn run_sql_script(conn: &mut DbConnection, script: &str) -> TestResult<()> {
    for statement in script
        .split(';')
        .map(str::trim)
        .filter(|sql| !sql.is_empty())
    {
        sql_query(statement).execute(conn).await?;
    }
    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn assert_upgrade_backfills(conn: &mut DbConnection) -> TestResult<()> {
    let bundle = sql_query("SELECT guid, created_at FROM news_bundles WHERE id = 1")
        .get_result::<BundleBackfillRow>(conn)
        .await?;
    assert!(bundle.guid.is_some());
    assert!(bundle.created_at.is_some());

    let category =
        sql_query("SELECT guid, add_sn, delete_sn, created_at FROM news_categories WHERE id = 1")
            .get_result::<CategoryBackfillRow>(conn)
            .await?;
    assert!(category.guid.is_some());
    assert_eq!(category.add_sn, Some(1));
    assert_eq!(category.delete_sn, Some(0));
    assert!(category.created_at.is_some());
    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn insert_legacy_seed_data(conn: &mut DbConnection) -> TestResult<()> {
    run_statements(
        conn,
        &[
            "INSERT INTO __diesel_schema_migrations (version) VALUES ('00000000000000'), \
             ('00000000000001'), ('00000000000002'), ('00000000000003'), ('00000000000004'), \
             ('00000000000005')",
            "INSERT INTO users (id, username, password) VALUES (1, 'alice', 'hash')",
            "INSERT INTO news_bundles (id, parent_bundle_id, name) VALUES (1, NULL, 'Bundle')",
            "INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'General', 1)",
            "INSERT INTO news_articles (id, category_id, parent_article_id, prev_article_id, \
             next_article_id, first_child_article_id, title, poster, posted_at, flags, \
             data_flavor, data) VALUES (1, 1, NULL, NULL, NULL, NULL, 'First', 'alice', \
             '2026-04-13 00:00:00', 0, 'text/plain', 'hello')",
        ],
    )
    .await
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn setup_legacy_schema(
    conn: &mut DbConnection,
    migration_scripts: &[&str],
) -> TestResult<()> {
    run_sql_script(
        conn,
        "CREATE TABLE __diesel_schema_migrations (version VARCHAR(50) PRIMARY KEY NOT NULL, \
         run_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)",
    )
    .await?;
    for script in migration_scripts {
        run_sql_script(conn, script).await?;
    }
    insert_legacy_seed_data(conn).await
}

#[cfg(feature = "sqlite")]
async fn setup_sqlite_legacy_schema(conn: &mut DbConnection) -> TestResult<()> {
    setup_legacy_schema(
        conn,
        &[
            include_str!("../../migrations/sqlite/00000000000000_create_users/up.sql"),
            include_str!("../../migrations/sqlite/00000000000001_create_news/up.sql"),
            include_str!("../../migrations/sqlite/00000000000002_add_bundles/up.sql"),
            include_str!("../../migrations/sqlite/00000000000003_add_articles/up.sql"),
            include_str!("../../migrations/sqlite/00000000000004_create_files/up.sql"),
            include_str!(
                "../../migrations/sqlite/00000000000005_add_bundle_name_parent_index/up.sql"
            ),
        ],
    )
    .await
}

#[cfg(feature = "sqlite")]
async fn sqlite_names(conn: &mut DbConnection, sql: &str) -> TestResult<Vec<String>> {
    Ok(sql_query(sql)
        .load::<NameRow>(conn)
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect())
}

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "sqlite")]
async fn assert_sqlite_news_schema(conn: &mut DbConnection) -> TestResult<()> {
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

#[cfg(feature = "postgres")]
async fn assert_permission_round_trip(conn: &mut DbConnection) -> TestResult<()> {
    run_statements(
        conn,
        &[
            "INSERT INTO users (id, username, password) VALUES (42, 'schema-user', 'hash')",
            "INSERT INTO permissions (id, code, name, scope) VALUES (42, 34, 'News Create \
             Category', 'bundle')",
            "INSERT INTO user_permissions (user_id, permission_id) VALUES (42, 42)",
        ],
    )
    .await?;

    let permissions = sql_query(
        "SELECT COUNT(*) AS count FROM permissions p INNER JOIN user_permissions up ON \
         up.permission_id = p.id WHERE p.code = 34 AND p.scope = 'bundle' AND up.user_id = 42",
    )
    .get_result::<CountRow>(conn)
    .await?;
    assert_eq!(permissions.count, 1);
    Ok(())
}

#[cfg(feature = "sqlite")]
async fn assert_sqlite_aligned_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_sqlite_permission_schema(conn).await?;
    assert_sqlite_news_schema(conn).await
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_fresh_migration_creates_aligned_schema() -> TestResult<()> {
    let mut conn = sqlite_conn().await?;

    assert_sqlite_aligned_schema(&mut conn).await
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_upgrade_backfills_legacy_news_rows() -> TestResult<()> {
    let mut conn = DbConnection::establish(":memory:").await?;
    setup_sqlite_legacy_schema(&mut conn).await?;
    apply_migrations(&mut conn, "").await?;

    assert_upgrade_backfills(&mut conn).await?;
    assert_sqlite_aligned_schema(&mut conn).await
}

#[cfg(feature = "postgres")]
async fn setup_postgres_legacy_schema(conn: &mut DbConnection) -> TestResult<()> {
    setup_legacy_schema(
        conn,
        &[
            include_str!("../../migrations/postgres/00000000000000_create_users/up.sql"),
            include_str!("../../migrations/postgres/00000000000001_create_news/up.sql"),
            include_str!("../../migrations/postgres/00000000000002_add_bundles/up.sql"),
            include_str!("../../migrations/postgres/00000000000003_add_articles/up.sql"),
            include_str!("../../migrations/postgres/00000000000004_create_files/up.sql"),
            include_str!(
                "../../migrations/postgres/00000000000005_add_bundle_name_parent_index/up.sql"
            ),
        ],
    )
    .await
}

#[cfg(feature = "postgres")]
async fn postgres_names(conn: &mut DbConnection, sql: &str) -> TestResult<Vec<String>> {
    Ok(sql_query(sql)
        .load::<NameRow>(conn)
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect())
}

#[cfg(feature = "postgres")]
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
    for expected in [
        "idx_user_permissions_perm",
        "idx_user_permissions_user",
        "permissions_code_key",
        "user_permissions_pkey",
    ] {
        assert!(permission_indices.iter().any(|name| name == expected));
    }
    Ok(())
}

#[cfg(feature = "postgres")]
async fn assert_postgres_news_schema(conn: &mut DbConnection) -> TestResult<()> {
    let bundle_columns = postgres_names(
        conn,
        "SELECT column_name AS name FROM information_schema.columns WHERE table_name = \
         'news_bundles' ORDER BY ordinal_position",
    )
    .await?;
    assert_eq!(
        bundle_columns,
        vec!["id", "parent_bundle_id", "name", "guid", "created_at"]
    );

    let category_columns = postgres_names(
        conn,
        "SELECT column_name AS name FROM information_schema.columns WHERE table_name = \
         'news_categories' ORDER BY ordinal_position",
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

    let article_indices = postgres_names(
        conn,
        "SELECT indexname AS name FROM pg_indexes WHERE tablename = 'news_articles' ORDER BY \
         indexname",
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

#[cfg(feature = "postgres")]
async fn assert_postgres_aligned_schema(conn: &mut DbConnection) -> TestResult<()> {
    assert_postgres_permission_schema(conn).await?;
    assert_postgres_news_schema(conn).await
}

#[cfg(feature = "postgres")]
fn should_skip_embedded_postgres() -> bool {
    if std::env::var_os("POSTGRES_TEST_URL").is_some() {
        tracing::warn!("SKIP-TEST-CLUSTER: POSTGRES_TEST_URL set, skipping embedded postgres test");
        return true;
    }
    false
}

#[cfg(feature = "postgres")]
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

#[cfg(feature = "postgres")]
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

#[cfg(feature = "postgres")]
#[test]
fn postgres_fresh_migration_creates_aligned_schema() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url).await?;

        assert_postgres_aligned_schema(&mut conn).await?;
        assert_permission_round_trip(&mut conn).await
    })
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_upgrade_backfills_legacy_news_rows() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        setup_postgres_legacy_schema(&mut conn).await?;
        apply_migrations(&mut conn, &url).await?;

        assert_upgrade_backfills(&mut conn).await?;
        assert_postgres_aligned_schema(&mut conn).await
    })
}

#[cfg(feature = "postgres")]
fn embedded_postgres_db() -> TestResult<Option<PostgresTestDb>> {
    if should_skip_embedded_postgres() {
        return Ok(None);
    }
    start_embedded_postgres_db()
}
