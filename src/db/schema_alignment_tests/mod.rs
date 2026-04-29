//! Regression tests for news schema alignment migrations.
//!
//! These tests validate both fresh schema application and upgrades from the
//! pre-4.1.1 news schema so later roadmap work can rely on the aligned
//! persistence contract.

use chrono::NaiveDateTime;
use diesel::{
    QueryableByName,
    sql_query,
    sql_types::{Integer, Nullable, Text, Timestamp},
};
use diesel_async::RunQueryDsl;

use super::{DbConnection, apply_migrations};

#[cfg(feature = "postgres")]
mod postgres_tests;
#[cfg(feature = "sqlite")]
mod sqlite_tests;

pub(crate) type TestResult<T> = Result<T, anyhow::Error>;

#[derive(QueryableByName)]
pub(crate) struct NameRow {
    #[diesel(sql_type = Text)]
    pub(crate) name: String,
}

#[derive(QueryableByName)]
pub(crate) struct CountRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub(crate) count: i64,
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

#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn run_statements(conn: &mut DbConnection, statements: &[&str]) -> TestResult<()> {
    for &statement in statements {
        sql_query(statement).execute(conn).await?;
    }
    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn run_sql_script(conn: &mut DbConnection, script: &str) -> TestResult<()> {
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
pub(crate) async fn assert_bundle_backfill(conn: &mut DbConnection) -> TestResult<()> {
    let bundle = sql_query("SELECT guid, created_at FROM news_bundles WHERE id = 1")
        .get_result::<BundleBackfillRow>(conn)
        .await?;
    assert!(bundle.guid.is_some());
    assert!(bundle.created_at.is_some());
    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn assert_category_backfill(conn: &mut DbConnection) -> TestResult<()> {
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
pub(crate) async fn assert_upgrade_backfills(conn: &mut DbConnection) -> TestResult<()> {
    assert_bundle_backfill(conn).await?;
    assert_category_backfill(conn).await?;
    assert_permission_round_trip_with_ids(conn, 84, 84, 84).await?;
    #[cfg(feature = "sqlite")]
    sqlite_tests::assert_sqlite_article_indices(conn).await?;
    #[cfg(feature = "postgres")]
    postgres_tests::assert_postgres_article_indices(conn).await?;
    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn assert_root_category_names_are_unique(
    conn: &mut DbConnection,
) -> TestResult<()> {
    sql_query(
        "INSERT INTO news_categories (id, bundle_id, name) VALUES (9001, NULL, 'Root Duplicate')",
    )
    .execute(conn)
    .await?;

    let duplicate = sql_query(
        "INSERT INTO news_categories (id, bundle_id, name) VALUES (9002, NULL, 'Root Duplicate')",
    )
    .execute(conn)
    .await;
    assert!(duplicate.is_err());
    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn insert_legacy_seed_data(conn: &mut DbConnection) -> TestResult<()> {
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
pub(crate) async fn setup_legacy_schema(
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

#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn assert_permission_round_trip_with_ids(
    conn: &mut DbConnection,
    user_id: i32,
    permission_id: i32,
    code: i32,
) -> TestResult<()> {
    for statement in [
        format!(
            "INSERT INTO users (id, username, password) VALUES ({user_id}, \
             'schema-user-{user_id}', 'hash')"
        ),
        format!(
            "INSERT INTO permissions (id, code, name, scope) VALUES ({permission_id}, {code}, \
             'News Create Category {code}', 'bundle')"
        ),
        format!(
            "INSERT INTO user_permissions (user_id, permission_id) VALUES ({user_id}, \
             {permission_id})"
        ),
    ] {
        sql_query(statement).execute(conn).await?;
    }

    let permissions = sql_query(format!(
        "SELECT COUNT(*) AS count FROM permissions p INNER JOIN user_permissions up ON \
         up.permission_id = p.id WHERE p.code = {code} AND p.scope = 'bundle' AND up.user_id = \
         {user_id}"
    ))
    .get_result::<CountRow>(conn)
    .await?;
    assert_eq!(permissions.count, 1);
    Ok(())
}
