//! Permission cascade tests (`PostgreSQL`).
//!
//! Scope:
//! - Asserts cascade deletion from `permissions` and from `users` to the `user_permissions` join
//!   table; non-deleted principals remain.
//!
//! Utilities and execution model:
//! - Runs against embedded or `POSTGRES_TEST_URL` connections and is serialised with
//!   `serial_test::file_serial(postgres_embedded_setup)` to avoid shared cluster race conditions.
//!
//! CQRS structure:
//! - Command helpers (`delete_permission`, `delete_user`) isolate mutations.
//! - Query helpers (`assignment_count`, `user_exists`, `permission_code`) isolate reads.
//! - Predicates (`assignment_is_empty`, `user_is_absent`, `permission_code_matches`) compose
//!   queries into boolean results.
//! - The test orchestrates: seed → command → assert predicate.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::{
    db::{DbConnection, create_user, get_user_by_name},
    models::{NewPermission, NewUser, NewUserPermission},
    schema::{
        permissions::dsl as permissions,
        user_permissions::dsl as user_permissions,
        users::dsl as users,
    },
};

type TestResult<T> = anyhow::Result<T>;

/// Seeds a permission with `code` and `name`, creates a join row for `user_id`,
/// and returns the new permission's `id`.
async fn insert_permission_assignment(
    conn: &mut DbConnection,
    user_id: i32,
    code: i32,
    name: &str,
) -> TestResult<i32> {
    let permission = NewPermission {
        code,
        name,
        description: "PostgreSQL permission cascade test permission",
    };
    diesel::insert_into(permissions::permissions)
        .values(&permission)
        .execute(conn)
        .await?;
    let permission_id = permissions::permissions
        .filter(permissions::code.eq(code))
        .select(permissions::id)
        .first::<i32>(conn)
        .await?;

    let user_permission = NewUserPermission {
        user_id,
        permission_id,
    };
    diesel::insert_into(user_permissions::user_permissions)
        .values(&user_permission)
        .execute(conn)
        .await?;
    Ok(permission_id)
}

/// Deletes the permission row identified by `permission_id`.
async fn delete_permission(conn: &mut DbConnection, permission_id: i32) -> TestResult<()> {
    diesel::delete(permissions::permissions.filter(permissions::id.eq(permission_id)))
        .execute(conn)
        .await?;
    Ok(())
}

/// Deletes the user row identified by `user_id`.
async fn delete_user(conn: &mut DbConnection, user_id: i32) -> TestResult<()> {
    diesel::delete(users::users.filter(users::id.eq(user_id)))
        .execute(conn)
        .await?;
    Ok(())
}

/// Returns the count of `user_permissions` rows matching `user_id` and
/// `permission_id`.
async fn assignment_count(
    conn: &mut DbConnection,
    user_id: i32,
    permission_id: i32,
) -> TestResult<i64> {
    user_permissions::user_permissions
        .filter(user_permissions::user_id.eq(user_id))
        .filter(user_permissions::permission_id.eq(permission_id))
        .count()
        .get_result::<i64>(conn)
        .await
        .map_err(anyhow::Error::from)
}

/// Returns `true` when no `user_permissions` row matches `user_id` and
/// `permission_id`.
async fn assignment_is_empty(
    conn: &mut DbConnection,
    user_id: i32,
    permission_id: i32,
) -> TestResult<bool> {
    Ok(assignment_count(conn, user_id, permission_id).await? == 0)
}

/// Returns the count of `users` rows matching `user_id`.
async fn user_row_count(conn: &mut DbConnection, user_id: i32) -> TestResult<i64> {
    users::users
        .filter(users::id.eq(user_id))
        .count()
        .get_result::<i64>(conn)
        .await
        .map_err(anyhow::Error::from)
}

/// Returns `true` when no `users` row exists for `user_id`.
async fn user_is_absent(conn: &mut DbConnection, user_id: i32) -> TestResult<bool> {
    Ok(user_row_count(conn, user_id).await? == 0)
}

/// Returns the `code` value for the permission row identified by
/// `permission_id`.
async fn permission_code(conn: &mut DbConnection, permission_id: i32) -> TestResult<i32> {
    permissions::permissions
        .filter(permissions::id.eq(permission_id))
        .select(permissions::code)
        .first::<i32>(conn)
        .await
        .map_err(anyhow::Error::from)
}

/// Returns `true` when the `code` column of the permission identified by
/// `permission_id` equals `expected_code`.
async fn permission_code_matches(
    conn: &mut DbConnection,
    permission_id: i32,
    expected_code: i32,
) -> TestResult<bool> {
    Ok(permission_code(conn, permission_id).await? == expected_code)
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[serial_test::file_serial(postgres_embedded_setup)]
#[test]
fn test_user_permission_cascades() -> TestResult<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(super::file_node_tests::with_embedded_pg(
        "permission_cascade",
        |conn| {
            Box::pin(async move {
                let user = NewUser {
                    username: "postgres-dana",
                    password: "hash",
                };
                create_user(conn, &user).await?;
                let stored_user = get_user_by_name(conn, "postgres-dana")
                    .await
                    .map_err(anyhow::Error::from)?
                    .ok_or_else(|| anyhow::anyhow!("postgres permission test user missing"))?;

                // Phase 1: delete permission, confirm assignment cascade, confirm user remains.
                let deleted_permission_id = insert_permission_assignment(
                    conn,
                    stored_user.id,
                    3401,
                    "News Create Category",
                )
                .await?;
                delete_permission(conn, deleted_permission_id).await?;
                assert!(
                    assignment_is_empty(conn, stored_user.id, deleted_permission_id).await?,
                    "permission deletion left assignments behind"
                );
                anyhow::ensure!(
                    !user_is_absent(conn, stored_user.id).await?,
                    "user row unexpectedly removed after permission deletion"
                );

                // Phase 2: delete user, confirm assignment cascade, confirm permission remains.
                let retained_permission_id = insert_permission_assignment(
                    conn,
                    stored_user.id,
                    3402,
                    "News Delete Category",
                )
                .await?;
                delete_user(conn, stored_user.id).await?;
                assert!(
                    assignment_is_empty(conn, stored_user.id, retained_permission_id).await?,
                    "cascade left assignments behind after user deletion"
                );
                assert!(
                    user_is_absent(conn, stored_user.id).await?,
                    "deleted user row remains"
                );
                assert!(
                    permission_code_matches(conn, retained_permission_id, 3402).await?,
                    "permission changed after user deletion"
                );
                Ok(())
            })
        },

}
