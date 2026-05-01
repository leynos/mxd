//! `PostgreSQL` schema alignment regression tests for roadmap item 4.1.1.
//!
//! Validates that both a fresh application of the current migration set and an
//! upgrade from the pre-4.1.1 schema produce the expected table structure,
//! column order, indexes, constraints, and backfill values on the `PostgreSQL`
//! backend.  Tests are run against either a `POSTGRES_TEST_URL` connection or
//! an embedded `PostgreSQL` cluster started by `pg-embed-setup-unpriv`; they
//! skip gracefully when neither is available.  All test functions are
//! serialised via `serial_test::file_serial` to prevent concurrent access to
//! the shared embedded cluster.

mod catalogue_helpers;
mod threading;

use diesel::sql_query;
use diesel_async::{AsyncConnection, RunQueryDsl};

pub(super) use self::catalogue_helpers::assert_postgres_article_indices;
use self::catalogue_helpers::{
    assert_permission_round_trip,
    assert_postgres_aligned_schema,
    postgres_names,
    setup_postgres_legacy_schema,
    with_postgres_test_db,
};
use super::{DbConnection, TestResult, apply_migrations, assert_upgrade_backfills};

#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_fresh_migration_creates_aligned_schema() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        assert_postgres_aligned_schema(&mut conn).await?;
        assert_permission_round_trip(&mut conn).await
    })
}

#[serial_test::file_serial(postgres_embedded_setup)]
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

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_category_names_are_bundle_scoped() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        sql_query("INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'BundleA')")
            .execute(&mut conn)
            .await?;
        sql_query("INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'BundleB')")
            .execute(&mut conn)
            .await?;

        let bundle_ids = postgres_names(
            &mut conn,
            "SELECT id::text AS name FROM news_bundles ORDER BY id",
        )
        .await?;
        let (bid1, bid2) = (&bundle_ids[0], &bundle_ids[1]);

        // Same name in different bundles must succeed
        sql_query(format!(
            "INSERT INTO news_categories (name, bundle_id) VALUES ('Sports', {bid1})"
        ))
        .execute(&mut conn)
        .await?;
        sql_query(format!(
            "INSERT INTO news_categories (name, bundle_id) VALUES ('Sports', {bid2})"
        ))
        .execute(&mut conn)
        .await?;

        // Same name in the same bundle must fail
        let duplicate = sql_query(format!(
            "INSERT INTO news_categories (name, bundle_id) VALUES ('Sports', {bid1})"
        ))
        .execute(&mut conn)
        .await;
        assert!(
            duplicate.is_err(),
            "duplicate name in same bundle must be rejected"
        );
        Ok(())
    })
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_guids_are_non_empty_and_unique() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        sql_query(
            "INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'GA'), (NULL, 'GB')",
        )
        .execute(&mut conn)
        .await?;

        let guids = postgres_names(
            &mut conn,
            "SELECT guid AS name FROM news_bundles ORDER BY id",
        )
        .await?;
        assert_eq!(guids.len(), 2, "expected two bundle rows");
        for guid in &guids {
            assert!(!guid.is_empty(), "GUID must not be empty");
        }
        assert_ne!(guids[0], guids[1], "GUIDs must be unique across rows");
        Ok(())
    })
}
