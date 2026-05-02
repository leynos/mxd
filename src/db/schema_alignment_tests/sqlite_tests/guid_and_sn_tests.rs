//! `SQLite` GUID and serial-number behaviour tests.
//!
//! Validates bundle-scoped category name uniqueness, GUID non-emptiness and
//! uniqueness across bundles and categories, and `add_sn` default behaviour
//! for fresh inserts.

use anyhow::Context;
use diesel_async::RunQueryDsl;
use rstest::rstest;

use super::{DbConnection, TestResult, add_sn_db, sqlite_names, two_bundle_db};

#[rstest]
#[tokio::test]
async fn sqlite_category_names_are_bundle_scoped(
    #[future] two_bundle_db: TestResult<DbConnection>,
) -> TestResult<()> {
    let mut conn = two_bundle_db.await?;

    // Same name in different bundles must succeed
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'Sports', 1)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert first SQLite scoped Sports category")?;
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (2, 'Sports', 2)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert second SQLite scoped Sports category")?;

    // Same name in the same bundle must fail
    let duplicate = diesel::sql_query(
        "INSERT INTO news_categories (id, name, bundle_id) VALUES (3, 'Sports', 1)",
    )
    .execute(&mut conn)
    .await;
    anyhow::ensure!(
        duplicate.is_err(),
        "duplicate name in same bundle must be rejected"
    );
    Ok(())
}

#[rstest]
#[tokio::test]
async fn sqlite_guids_are_non_empty_and_unique(
    #[future] two_bundle_db: TestResult<DbConnection>,
) -> TestResult<()> {
    let mut conn = two_bundle_db.await?;

    let guids = sqlite_names(
        &mut conn,
        "SELECT guid AS name FROM news_bundles ORDER BY id",
    )
    .await?;
    anyhow::ensure!(guids.len() == 2, "expected two bundle rows");
    for guid in &guids {
        anyhow::ensure!(!guid.is_empty(), "GUID must not be empty");
    }
    let guid_set: std::collections::HashSet<_> = guids.iter().collect();
    anyhow::ensure!(
        guid_set.len() == guids.len(),
        "GUIDs must be unique across rows"
    );

    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (1, 'CA', 1)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert first SQLite GUID category")?;
    diesel::sql_query("INSERT INTO news_categories (id, name, bundle_id) VALUES (2, 'CB', 2)")
        .execute(&mut conn)
        .await
        .context("EXECUTE insert second SQLite GUID category")?;

    let category_guids = sqlite_names(
        &mut conn,
        "SELECT guid AS name FROM news_categories ORDER BY id",
    )
    .await?;
    anyhow::ensure!(category_guids.len() == 2, "expected two category rows");
    for guid in &category_guids {
        anyhow::ensure!(!guid.is_empty(), "category GUID must not be empty");
    }
    let category_guid_set: std::collections::HashSet<_> = category_guids.iter().collect();
    anyhow::ensure!(
        category_guid_set.len() == category_guids.len(),
        "category GUIDs must be unique"
    );
    Ok(())
}

#[rstest]
#[tokio::test]
async fn sqlite_add_sn_defaults_to_zero_for_fresh_inserts(
    #[future] add_sn_db: TestResult<DbConnection>,
) -> TestResult<()> {
    let mut conn = add_sn_db.await?;

    let add_sn_row = diesel::sql_query("SELECT add_sn AS name FROM news_categories WHERE id = 1")
        .get_result::<super::NameRow>(&mut conn)
        .await
        .context("LOAD SQLite add_sn for category id=1")?;
    let add_sn_1: i32 = add_sn_row
        .name
        .parse()
        .context("failed to parse add_sn for id=1")?;
    anyhow::ensure!(
        add_sn_1 == 0,
        "add_sn is set at migration time; fresh inserts do not auto-increment it"
    );

    let add_sn_empty_row =
        diesel::sql_query("SELECT add_sn AS name FROM news_categories WHERE id = 2")
            .get_result::<super::NameRow>(&mut conn)
            .await
            .context("LOAD SQLite add_sn for category id=2")?;
    let add_sn_empty: i32 = add_sn_empty_row
        .name
        .parse()
        .context("failed to parse add_sn for id=2")?;
    anyhow::ensure!(add_sn_empty == 0, "empty category add_sn must be 0");

    Ok(())
}