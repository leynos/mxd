//! `SQLite` fresh migration and legacy upgrade-flow tests.

#![cfg(feature = "sqlite")]

use diesel_async::AsyncConnection;

use super::{
    super::{
        DbConnection,
        TestResult,
        apply_migrations,
        assert_upgrade_backfills_readonly,
        seed_upgrade_backfills,
    },
    common::{setup_sqlite_legacy_schema, sqlite_conn},
    schema_catalogue::assert_sqlite_aligned_schema,
};

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

    seed_upgrade_backfills(&mut conn).await?;
    assert_upgrade_backfills_readonly(&mut conn).await?;
    assert_sqlite_aligned_schema(&mut conn).await
}
