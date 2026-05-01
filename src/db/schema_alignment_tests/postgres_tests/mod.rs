//! `PostgreSQL` schema-alignment regression tests for roadmap item 4.1.1.
//!
//! Scope:
//! - Validates aligned table/column order, partial unique indices for root categories, and
//!   article-threading indices on fresh migration and on upgrade from legacy schema.
//! - Exercises permission round-trips using fixed IDs via helper flows.
//!
//! Helper interdependencies:
//! - Uses `catalogue_helpers` for schema catalogue queries (tables/columns/indices),
//!   `with_postgres_test_db` for embedded/URL-backed database setup, and `assert_upgrade_backfills`
//!   from the parent module for cross-backend backfill checks.

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

        let bundle_ids = postgres_names(
            &mut conn,
            "SELECT id::text AS name FROM news_bundles ORDER BY id",
        )
        .await?;
        let bid1 = bundle_ids
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no bundle rows found"))?;

        let bundle_guids = postgres_names(
            &mut conn,
            "SELECT guid AS name FROM news_bundles ORDER BY id",
        )
        .await?;
        assert_eq!(bundle_guids.len(), 2, "expected two bundle rows");
        for guid in &bundle_guids {
            assert!(!guid.is_empty(), "bundle GUID must not be empty");
        }
        assert_ne!(
            bundle_guids[0], bundle_guids[1],
            "bundle GUIDs must be unique across rows"
        );

        sql_query(format!(
            "INSERT INTO news_categories (name, bundle_id) VALUES ('CatAlpha', {bid1})"
        ))
        .execute(&mut conn)
        .await?;
        let bid2 = bundle_ids
            .get(1)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("expected second bundle row"))?;
        sql_query(format!(
            "INSERT INTO news_categories (name, bundle_id) VALUES ('CatBeta', {bid2})"
        ))
        .execute(&mut conn)
        .await?;

        let category_guids = postgres_names(
            &mut conn,
            "SELECT guid AS name FROM news_categories ORDER BY id",
        )
        .await?;
        assert_eq!(category_guids.len(), 2, "expected two category rows");
        for guid in &category_guids {
            assert!(!guid.is_empty(), "category GUID must not be empty");
        }
        assert_ne!(
            category_guids[0], category_guids[1],
            "category GUIDs must be unique across rows"
        );
        Ok(())
    })
}
