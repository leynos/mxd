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

pub(super) use self::catalogue_helpers::postgres_article_indices_are_present;
use self::catalogue_helpers::{
    permission_round_trip_is_seeded_and_valid,
    postgres_aligned_schema_is_valid,
    postgres_names,
    setup_postgres_legacy_schema,
    with_postgres_test_db,
};
use super::{DbConnection, TestResult, apply_migrations, assert_upgrade_backfills};

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_fresh_migration_creates_aligned_schema() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        assert!(
            postgres_aligned_schema_is_valid(&mut conn).await?,
            "aligned schema not present after fresh migration"
        );
        assert!(
            permission_round_trip_is_seeded_and_valid(&mut conn).await?,
            "permission round-trip predicate failed after fresh migration"
        );
        Ok(())
    })
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_upgrade_backfills_legacy_news_rows() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        setup_postgres_legacy_schema(&mut conn).await?;
        apply_migrations(&mut conn, &url, None).await?;

        assert_upgrade_backfills(&mut conn).await?;
        assert!(
            postgres_aligned_schema_is_valid(&mut conn).await?,
            "aligned schema not present after legacy upgrade"
        );
        Ok(())
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
        let bid1 = bundle_ids
            .as_slice()
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing bundle id 1"))?;
        let bid2 = bundle_ids
            .get(1)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing bundle id 2"))?;

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

async fn seed_bundles_for_guid_test(conn: &mut DbConnection) -> TestResult<()> {
    sql_query(
        "INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'GA'), (NULL, 'GB')",
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns `true` when the expected number of bundle rows each have a non-empty
/// `guid` and a non-empty `created_at` value.
async fn bundle_guids_and_created_at_are_valid(
    conn: &mut DbConnection,
    expected_rows: usize,
) -> TestResult<bool> {
    let guids = postgres_names(conn, "SELECT guid AS name FROM news_bundles ORDER BY id").await?;
    if guids.len() != expected_rows {
        return Ok(false);
    }
    if guids.iter().any(|guid| guid.is_empty()) {
        return Ok(false);
    }
    let guid_set: std::collections::HashSet<_> = guids.iter().collect();
    if guid_set.len() != guids.len() {
        return Ok(false);
    }

    let bundle_created_at = postgres_names(
        conn,
        "SELECT created_at::text AS name FROM news_bundles ORDER BY id",
    )
    .await?;
    if bundle_created_at.len() != expected_rows {
        return Ok(false);
    }
    Ok(bundle_created_at.iter().all(|ts| !ts.is_empty()))
}

async fn fetch_two_bundle_ids(conn: &mut DbConnection) -> TestResult<(String, String)> {
    let bundle_ids = postgres_names(
        conn,
        "SELECT id::text AS name FROM news_bundles ORDER BY id",
    )
    .await?;
    let bid1 = bundle_ids
        .as_slice()
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing bundle id 1"))?;
    let bid2 = bundle_ids
        .get(1)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing bundle id 2"))?;
    Ok((bid1, bid2))
}

async fn seed_categories_for_guid_test(
    conn: &mut DbConnection,
    bid1: &str,
    bid2: &str,
) -> TestResult<()> {
    sql_query(format!(
        "INSERT INTO news_categories (name, bundle_id) VALUES ('CA', {bid1}), ('CB', {bid2})"
    ))
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns `true` when the expected number of category rows each have a
/// non-empty, unique `guid` and a non-empty `created_at` value.
async fn category_guids_and_created_at_are_valid(
    conn: &mut DbConnection,
    expected_rows: usize,
) -> TestResult<bool> {
    let category_guids =
        postgres_names(conn, "SELECT guid AS name FROM news_categories ORDER BY id").await?;
    if category_guids.len() != expected_rows {
        return Ok(false);
    }
    if category_guids.iter().any(|guid| guid.is_empty()) {
        return Ok(false);
    }
    let category_guid_set: std::collections::HashSet<_> = category_guids.iter().collect();
    if category_guid_set.len() != category_guids.len() {
        return Ok(false);
    }

    let category_created_at = postgres_names(
        conn,
        "SELECT created_at::text AS name FROM news_categories ORDER BY id",
    )
    .await?;
    if category_created_at.len() != expected_rows {
        return Ok(false);
    }
    Ok(category_created_at.iter().all(|ts| !ts.is_empty()))
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_guids_are_non_empty_and_unique() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        seed_bundles_for_guid_test(&mut conn).await?;
        assert!(
            bundle_guids_and_created_at_are_valid(&mut conn, 2).await?,
            "bundle GUIDs or created_at invalid after insert"
        );
        let (bid1, bid2) = fetch_two_bundle_ids(&mut conn).await?;
        seed_categories_for_guid_test(&mut conn, &bid1, &bid2).await?;
        assert!(
            category_guids_and_created_at_are_valid(&mut conn, 2).await?,
            "category GUIDs or created_at invalid after insert"
        );
        Ok(())
    })
}
