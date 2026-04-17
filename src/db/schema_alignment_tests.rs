//! Regression tests for news schema alignment migrations.
//!
//! These tests validate both fresh schema application and upgrades from the
//! pre-4.1.1 news schema so later roadmap work can rely on the aligned
//! persistence contract.

#![expect(clippy::panic_in_result_fn, reason = "test assertions")]

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

#[cfg(feature = "sqlite")]
async fn setup_sqlite_legacy_schema(conn: &mut DbConnection) -> TestResult<()> {
    run_statements(
        conn,
        &[
            "CREATE TABLE __diesel_schema_migrations (version VARCHAR(50) PRIMARY KEY NOT NULL, \
             run_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, username TEXT NOT NULL \
             UNIQUE, password TEXT NOT NULL)",
            "CREATE TABLE news_categories (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT \
             NULL UNIQUE, bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE)",
            "CREATE TABLE news_bundles (id INTEGER PRIMARY KEY AUTOINCREMENT, parent_bundle_id \
             INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE, name TEXT NOT NULL, \
             UNIQUE(name, parent_bundle_id))",
            "CREATE INDEX idx_bundles_parent ON news_bundles(parent_bundle_id)",
            "CREATE INDEX idx_categories_bundle ON news_categories(bundle_id)",
            "CREATE TABLE news_articles (id INTEGER PRIMARY KEY AUTOINCREMENT, category_id \
             INTEGER NOT NULL REFERENCES news_categories(id) ON DELETE CASCADE, parent_article_id \
             INTEGER REFERENCES news_articles(id), prev_article_id INTEGER REFERENCES \
             news_articles(id), next_article_id INTEGER REFERENCES news_articles(id), \
             first_child_article_id INTEGER REFERENCES news_articles(id), title TEXT NOT NULL, \
             poster TEXT, posted_at DATETIME NOT NULL, flags INTEGER DEFAULT 0, data_flavor TEXT \
             DEFAULT 'text/plain', data TEXT, CHECK (category_id IS NOT NULL))",
            "CREATE INDEX idx_articles_category ON news_articles(category_id)",
            "CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, \
             object_key TEXT NOT NULL, size INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE file_acl (file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE \
             CASCADE, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, PRIMARY \
             KEY (file_id, user_id))",
            "CREATE INDEX idx_file_acl_user_file ON file_acl (user_id, file_id)",
            "CREATE INDEX idx_bundles_name_parent ON news_bundles(name, parent_bundle_id)",
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
#[tokio::test]
async fn sqlite_fresh_migration_creates_aligned_schema() -> TestResult<()> {
    let mut conn = sqlite_conn().await?;

    let tables = sqlite_names(
        &mut conn,
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('permissions', \
         'user_permissions') ORDER BY name",
    )
    .await?;
    assert_eq!(tables, vec!["permissions", "user_permissions"]);

    let article_indices = sqlite_names(
        &mut conn,
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
        &mut conn,
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

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_upgrade_backfills_legacy_news_rows() -> TestResult<()> {
    let mut conn = DbConnection::establish(":memory:").await?;
    setup_sqlite_legacy_schema(&mut conn).await?;
    apply_migrations(&mut conn, "").await?;

    assert_upgrade_backfills(&mut conn).await
}

#[cfg(feature = "postgres")]
async fn setup_postgres_legacy_schema(conn: &mut DbConnection) -> TestResult<()> {
    run_statements(
        conn,
        &[
            "CREATE TABLE __diesel_schema_migrations (version VARCHAR(50) PRIMARY KEY NOT NULL, \
             run_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE users (id INTEGER PRIMARY KEY GENERATED BY DEFAULT AS IDENTITY, \
             username TEXT NOT NULL UNIQUE, password TEXT NOT NULL)",
            "CREATE TABLE news_categories (id INTEGER PRIMARY KEY GENERATED BY DEFAULT AS \
             IDENTITY, name TEXT NOT NULL UNIQUE, bundle_id INTEGER REFERENCES news_bundles(id) \
             ON DELETE CASCADE)",
            "CREATE TABLE news_bundles (id INTEGER PRIMARY KEY GENERATED BY DEFAULT AS IDENTITY, \
             parent_bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE, name TEXT \
             NOT NULL, UNIQUE(name, parent_bundle_id))",
            "CREATE INDEX idx_bundles_parent ON news_bundles(parent_bundle_id)",
            "CREATE INDEX idx_categories_bundle ON news_categories(bundle_id)",
            "CREATE TABLE news_articles (id INTEGER PRIMARY KEY GENERATED BY DEFAULT AS IDENTITY, \
             category_id INTEGER NOT NULL REFERENCES news_categories(id) ON DELETE CASCADE, \
             parent_article_id INTEGER REFERENCES news_articles(id), prev_article_id INTEGER \
             REFERENCES news_articles(id), next_article_id INTEGER REFERENCES news_articles(id), \
             first_child_article_id INTEGER REFERENCES news_articles(id), title TEXT NOT NULL, \
             poster TEXT, posted_at TIMESTAMP NOT NULL, flags INTEGER DEFAULT 0, data_flavor TEXT \
             DEFAULT 'text/plain', data TEXT, CHECK (category_id IS NOT NULL))",
            "CREATE INDEX idx_articles_category ON news_articles(category_id)",
            "CREATE TABLE files (id INTEGER PRIMARY KEY GENERATED BY DEFAULT AS IDENTITY, name \
             TEXT NOT NULL UNIQUE, object_key TEXT NOT NULL, size BIGINT NOT NULL DEFAULT 0)",
            "CREATE TABLE file_acl (file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE \
             CASCADE, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, PRIMARY \
             KEY (file_id, user_id))",
            "CREATE INDEX idx_file_acl_user_file ON file_acl (user_id, file_id)",
            "CREATE INDEX idx_bundles_name_parent ON news_bundles(name, parent_bundle_id)",
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

        let category_columns = postgres_names(
            &mut conn,
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
            &mut conn,
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

        let permission_tables = postgres_names(
            &mut conn,
            "SELECT table_name AS name FROM information_schema.tables WHERE table_schema = \
             'public' AND table_name IN ('permissions', 'user_permissions') ORDER BY table_name",
        )
        .await?;
        assert_eq!(permission_tables, vec!["permissions", "user_permissions"]);
        Ok(())
    })
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_upgrade_backfills_legacy_news_rows() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        setup_postgres_legacy_schema(&mut conn).await?;
        apply_migrations(&mut conn, &url).await?;

        assert_upgrade_backfills(&mut conn).await
    })
}

#[cfg(feature = "postgres")]
fn embedded_postgres_db() -> TestResult<Option<PostgresTestDb>> {
    if should_skip_embedded_postgres() {
        return Ok(None);
    }
    start_embedded_postgres_db()
}
