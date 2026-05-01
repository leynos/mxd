//! Regression tests for news schema alignment migrations.
//!
//! These tests validate both fresh schema application and upgrades from the
//! pre-4.1.1 news schema so later roadmap work can rely on the aligned
//! persistence contract.

use anyhow::Context;
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

/// Convenience alias for fallible test operations in schema alignment tests.
pub(crate) type TestResult<T> = Result<T, anyhow::Error>;

/// A single-column row holding a `name` string, used when querying
/// schema-metadata tables (e.g. `information_schema.columns`, `sqlite_master`)
/// that return name lists.
#[derive(QueryableByName)]
pub(crate) struct NameRow {
    #[diesel(sql_type = Text)]
    pub(crate) name: String,
}

/// A single-column row holding an aggregate `COUNT(*)` result, used to assert
/// the number of matching rows after permission and join-table operations.
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

/// Groups the three integer identifiers required by permission round-trip helpers
/// so they can be passed as a single value rather than three separate primitives.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) struct PermissionTestIds {
    pub(crate) user_id: i32,
    pub(crate) permission_id: i32,
    pub(crate) code: i32,
}

/// Executes each SQL statement in `statements` against `conn` in order,
/// returning the first error encountered.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn run_statements(conn: &mut DbConnection, statements: &[&str]) -> TestResult<()> {
    for &statement in statements {
        sql_query(statement)
            .execute(conn)
            .await
            .with_context(|| format!("failed executing SQL fragment: {statement}"))?;
    }
    Ok(())
}

/// Splits `script` on `;`, trims whitespace, discards empty fragments, and
/// executes each fragment against `conn` in order.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn run_sql_script(conn: &mut DbConnection, script: &str) -> TestResult<()> {
    for statement in script
        .split(';')
        .map(str::trim)
        .filter(|sql| !sql.is_empty())
    {
        sql_query(statement)
            .execute(conn)
            .await
            .with_context(|| format!("failed executing SQL fragment: {statement}"))?;
    }
    Ok(())
}

/// Returns `true` when the `news_bundles` row with `id = 1` has a non-empty
/// `guid` and a non-null `created_at` after migration backfill.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn bundle_backfill_is_valid(conn: &mut DbConnection) -> TestResult<bool> {
    let bundle = sql_query("SELECT guid, created_at FROM news_bundles WHERE id = 1")
        .get_result::<BundleBackfillRow>(conn)
        .await?;
    Ok(bundle
        .guid
        .as_deref()
        .is_some_and(|guid| !guid.trim().is_empty())
        && bundle.created_at.is_some())
}

/// Returns `true` when the `news_categories` row with `id = 1` has a non-empty
/// `guid`, `add_sn = 1`, `delete_sn = 0`, and a non-null `created_at`.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn category_backfill_is_valid(conn: &mut DbConnection) -> TestResult<bool> {
    category_backfill_is_valid_for_id(conn, 1, 1).await
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn category_backfill_is_valid_for_id(
    conn: &mut DbConnection,
    category_id: i32,
    expected_add_sn: i32,
) -> TestResult<bool> {
    let category = sql_query(format!(
        "SELECT guid, add_sn, delete_sn, created_at FROM news_categories WHERE id = {category_id}"
    ))
    .get_result::<CategoryBackfillRow>(conn)
    .await?;
    Ok(category
        .guid
        .as_deref()
        .is_some_and(|guid| !guid.trim().is_empty())
        && category.add_sn == Some(expected_add_sn)
        && category.delete_sn == Some(0)
        && category.created_at.is_some())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn empty_category_backfill_is_valid(conn: &mut DbConnection) -> TestResult<bool> {
    category_backfill_is_valid_for_id(conn, 2, 0).await
}

/// Returns the count of rows matching the permission join query for `ids`.
///
/// This is a pure query; it makes no modifications to `conn`.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn permission_join_count(
    conn: &mut DbConnection,
    ids: PermissionTestIds,
) -> TestResult<i64> {
    let PermissionTestIds {
        user_id,
        permission_id,
        code,
    } = ids;
    let row = sql_query(format!(
        "SELECT COUNT(*) AS count FROM permissions p INNER JOIN user_permissions up ON \
         up.permission_id = p.id WHERE p.code = {code} AND p.description = 'News category \
         permission {code}' AND up.user_id = {user_id} AND up.permission_id = {permission_id}"
    ))
    .get_result::<CountRow>(conn)
    .await?;
    Ok(row.count)
}

/// Returns `true` when exactly one row in `permissions JOIN user_permissions`
/// matches `code`, `'News category permission {code}'`, and `user_id`.
///
/// This is a pure predicate; it makes no modifications to `conn`.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn permission_round_trip_is_valid(
    conn: &mut DbConnection,
    ids: PermissionTestIds,
) -> TestResult<bool> {
    let count = permission_join_count(
        conn,
        PermissionTestIds {
            user_id: ids.user_id,
            permission_id: ids.permission_id,
            code: ids.code,
        },
    )
    .await?;
    Ok(count == 1)
}

/// Runs all post-upgrade backfill checks using predicates, then asserts each
/// result. Validates bundle, category, permission round-trip, and
/// backend-specific article-index checks.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
#[expect(
    clippy::panic_in_result_fn,
    reason = "test assertion helper; assert! is the intended failure mode"
)]
pub(crate) async fn assert_upgrade_backfills(conn: &mut DbConnection) -> TestResult<()> {
    assert!(
        bundle_backfill_is_valid(conn).await?,
        "bundle backfill validation failed after upgrade"
    );
    assert!(
        category_backfill_is_valid(conn).await?,
        "category backfill validation failed after upgrade"
    );
    assert!(
        empty_category_backfill_is_valid(conn).await?,
        "empty category backfill validation failed after upgrade"
    );
    seed_permission_round_trip(
        conn,
        PermissionTestIds {
            user_id: 84,
            permission_id: 84,
            code: 84,
        },
    )
    .await?;
    assert!(
        permission_round_trip_is_valid(
            conn,
            PermissionTestIds {
                user_id: 84,
                permission_id: 84,
                code: 84,
            },
        )
        .await?,
        "permission round-trip predicate failed after upgrade"
    );
    #[cfg(feature = "sqlite")]
    assert!(
        sqlite_tests::sqlite_article_indices_are_present(conn).await?,
        "SQLite article indices missing after upgrade"
    );
    #[cfg(feature = "postgres")]
    assert!(
        postgres_tests::postgres_article_indices_are_present(conn).await?,
        "PostgreSQL article indices missing after upgrade"
    );
    Ok(())
}

/// Inserts a root category named `'Root Duplicate'` and verifies that a second
/// insert with the same name and a `NULL` `bundle_id` fails with a constraint
/// error, validating the partial unique index on root categories.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn verify_root_category_names_are_unique_with_constraint_insert(
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
    anyhow::ensure!(
        duplicate.is_err(),
        "Expected duplicate insert to fail due to unique constraint"
    );
    Ok(())
}

/// Inserts the six legacy migration-version rows, one user, one bundle, one
/// category, and one article required by the legacy-schema upgrade tests.
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
            "INSERT INTO news_categories (id, name, bundle_id) VALUES (2, 'Empty', 1)",
            "INSERT INTO news_articles (id, category_id, parent_article_id, prev_article_id, \
             next_article_id, first_child_article_id, title, poster, posted_at, flags, \
             data_flavor, data) VALUES (1, 1, NULL, NULL, NULL, NULL, 'First', 'alice', \
             '2026-04-13 00:00:00', 0, 'text/plain', 'hello')",
        ],
    )
    .await
}

/// Runs the `__diesel_schema_migrations` DDL, then each script in
/// `migration_scripts`, then inserts the shared legacy seed data.
///
/// Used by `setup_sqlite_legacy_schema` and `setup_postgres_legacy_schema` to
/// establish a pre-4.1.1 database state before exercising the upgrade path.
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

/// Inserts a user, a permission, and a `user_permissions` join row using the
/// supplied `user_id`, `permission_id`, and `code`.  Returns `Ok(())` on
/// success; all inserts are executed in statement order against `conn`.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn seed_permission_round_trip(
    conn: &mut DbConnection,
    ids: PermissionTestIds,
) -> TestResult<()> {
    let PermissionTestIds {
        user_id,
        permission_id,
        code,
    } = ids;
    run_statements(
        conn,
        &[
            &format!(
                "INSERT INTO users (id, username, password) VALUES ({user_id}, \
                 'schema-user-{user_id}', 'hash')"
            ),
            &format!(
                "INSERT INTO permissions (id, code, name, description) VALUES ({permission_id}, \
                 {code}, 'News Create Category {code}', 'News category permission {code}')"
            ),
            &format!(
                "INSERT INTO user_permissions (user_id, permission_id) VALUES ({user_id}, \
                 {permission_id})"
            ),
        ],
    )
    .await
}
