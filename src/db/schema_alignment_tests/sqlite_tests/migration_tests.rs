//! `SQLite` migration tests: fresh schema creation and legacy upgrade backfill.

use super::{
    DbConnection,
    PermissionTestIds,
    TestResult,
    apply_migrations,
    assert_sqlite_aligned_schema,
    assert_upgrade_backfills,
    seed_permission_round_trip,
    setup_sqlite_legacy_schema,
};
use diesel_async::AsyncConnection;

#[tokio::test]
async fn sqlite_fresh_migration_creates_aligned_schema() -> TestResult<()> {
    let mut conn = super::sqlite_conn().await?;

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