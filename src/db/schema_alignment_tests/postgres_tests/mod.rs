//! `PostgreSQL` schema-alignment regression tests for roadmap item 4.1.1.
//!
//! Scope:
//! - Validates aligned table/column order, partial unique indices for root categories, and
//!   article-threading indices on fresh migration and on upgrade from legacy schema.
//! - Exercises permission round-trips using fixed IDs via helper flows.
//!
//! Helper interdependencies:
//! - Uses `catalogue_helpers` for schema catalogue queries (tables/columns/indices),
//!   `with_postgres_test_db` for embedded/URL-backed database setup, and the parent module's
//!   cross-backend backfill seed/assertion helpers.

mod catalogue_helpers;
mod threading;

use anyhow::Context;
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
use super::{
    DbConnection,
    NameRow,
    TestResult,
    apply_migrations,
    assert_upgrade_backfills_readonly,
    seed_upgrade_backfills,
};

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

        seed_upgrade_backfills(&mut conn).await?;
        assert_upgrade_backfills_readonly(&mut conn).await?;
        assert_postgres_aligned_schema(&mut conn).await
    })
}

#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_category_names_are_bundle_scoped() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        let bid1 = sql_query(
            "INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'BundleA') RETURNING \
             id::text AS name",
        )
        .get_result::<NameRow>(&mut conn)
        .await
        .context("LOAD inserted BundleA id")?
        .name;
        let bid2 = sql_query(
            "INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'BundleB') RETURNING \
             id::text AS name",
        )
        .get_result::<NameRow>(&mut conn)
        .await
        .context("LOAD inserted BundleB id")?
        .name;

        // Same name in different bundles must succeed
        sql_query(format!(
            "INSERT INTO news_categories (name, bundle_id) VALUES ('Sports', {bid1})"
        ))
        .execute(&mut conn)
        .await
        .context("EXECUTE insert first scoped Sports category")?;
        sql_query(format!(
            "INSERT INTO news_categories (name, bundle_id) VALUES ('Sports', {bid2})"
        ))
        .execute(&mut conn)
        .await
        .context("EXECUTE insert second scoped Sports category")?;

        // Same name in the same bundle must fail
        let duplicate = sql_query(format!(
            "INSERT INTO news_categories (name, bundle_id) VALUES ('Sports', {bid1})"
        ))
        .execute(&mut conn)
        .await;
        anyhow::ensure!(
            matches!(
                duplicate,
                Err(diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::UniqueViolation,
                    _
                ))
            ),
            "duplicate name in same bundle must fail with a unique-constraint error"
        );
        Ok(())
    })
}

async fn seed_bundles_for_guid_test(conn: &mut DbConnection) -> TestResult<(String, String)> {
    let ga = sql_query(
        "INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'GA') RETURNING id::text \
         AS name",
    )
    .get_result::<NameRow>(conn)
    .await
    .context("LOAD inserted GA bundle id")?
    .name;
    let gb = sql_query(
        "INSERT INTO news_bundles (parent_bundle_id, name) VALUES (NULL, 'GB') RETURNING id::text \
         AS name",
    )
    .get_result::<NameRow>(conn)
    .await
    .context("LOAD inserted GB bundle id")?
    .name;
    Ok((ga, gb))
}

async fn assert_pg_guids_and_created_at(
    conn: &mut DbConnection,
    table: &str,
    expected_rows: usize,
    label: &str,
) -> TestResult<()> {
    let guid_sql = format!("SELECT guid AS name FROM {table} ORDER BY id");
    let guids = postgres_names(conn, &guid_sql).await?;
    anyhow::ensure!(
        guids.len() == expected_rows,
        "expected {expected_rows} {label} rows, got {actual_rows}",
        actual_rows = guids.len()
    );
    for guid in &guids {
        anyhow::ensure!(!guid.is_empty(), "{label} GUID must not be empty");
    }
    let guid_set: std::collections::HashSet<_> = guids.iter().collect();
    anyhow::ensure!(
        guid_set.len() == guids.len(),
        "{label} GUIDs must be unique across rows"
    );

    let created_at_sql = format!("SELECT created_at::text AS name FROM {table} ORDER BY id");
    let created_at_values = postgres_names(conn, &created_at_sql).await?;
    anyhow::ensure!(
        created_at_values.len() == expected_rows,
        "expected {expected_rows} {label} rows, got {actual_rows}",
        actual_rows = created_at_values.len()
    );
    for created_at in &created_at_values {
        anyhow::ensure!(
            !created_at.is_empty(),
            "{label} created_at must not be empty"
        );
    }
    Ok(())
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
    .await
    .context("EXECUTE insert categories for GUID test")?;
    Ok(())
}

#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn postgres_guids_are_non_empty_and_unique() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url, None).await?;

        let (bid1, bid2) = seed_bundles_for_guid_test(&mut conn).await?;
        assert_pg_guids_and_created_at(&mut conn, "news_bundles", 2, "bundle").await?;
        seed_categories_for_guid_test(&mut conn, &bid1, &bid2).await?;
        assert_pg_guids_and_created_at(&mut conn, "news_categories", 2, "category").await
    })
}
