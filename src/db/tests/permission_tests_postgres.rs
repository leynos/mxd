//! Permission cascade tests (`PostgreSQL`).
//!
//! Scope:
//! - Asserts cascade deletion from `permissions` and from `users` to the `user_permissions` join
//!   table; non-deleted principals remain.
//!
//! Utilities and execution model:
//! - Runs against embedded or `POSTGRES_TEST_URL` connections and is serialised with
//!   `serial_test::file_serial(postgres_embedded_setup)` to avoid shared cluster race conditions.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::{
    db::{DbConnection, create_user, get_user_by_name},
    models::{NewPermission, NewUser, NewUserPermission, Permission, UserPermission},
    schema::{
        permissions::dsl as permissions,
        user_permissions::dsl as user_permissions,
        users::dsl as users,
    },
};

type TestResult<T> = anyhow::Result<T>;

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

async fn assert_permission_delete_cascades_to_assignments(
    conn: &mut DbConnection,
    user_id: i32,
    permission_id: i32,
) -> TestResult<()> {
    diesel::delete(permissions::permissions.filter(permissions::id.eq(permission_id)))
        .execute(&mut *conn)
        .await?;

    let assignments = user_permissions::user_permissions
        .load::<UserPermission>(conn)
        .await?;
    anyhow::ensure!(
        assignments.is_empty(),
        "permission deletion left assignments behind"
    );

    users::users
        .filter(users::id.eq(user_id))
        .select(users::id)
        .first::<i32>(conn)
        .await?;
    Ok(())
}

async fn assert_user_delete_cascades_to_assignments(
    conn: &mut DbConnection,
    user_id: i32,
    permission_id: i32,
) -> TestResult<()> {
    diesel::delete(users::users.filter(users::id.eq(user_id)))
        .execute(&mut *conn)
        .await?;

    let assignments = user_permissions::user_permissions
        .load::<UserPermission>(&mut *conn)
        .await?;
    anyhow::ensure!(assignments.is_empty(), "cascade left assignments behind");

    let stored_permission = permissions::permissions
        .filter(permissions::id.eq(permission_id))
        .first::<Permission>(conn)
        .await?;
    anyhow::ensure!(
        stored_permission.code == 3402,
        "permission changed after user deletion"
    );
    Ok(())
}

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

                let deleted_permission_id = insert_permission_assignment(
                    conn,
                    stored_user.id,
                    3401,
                    "News Create Category",
                )
                .await?;
                assert_permission_delete_cascades_to_assignments(
                    conn,
                    stored_user.id,
                    deleted_permission_id,
                )
                .await?;

                let retained_permission_id = insert_permission_assignment(
                    conn,
                    stored_user.id,
                    3402,
                    "News Delete Category",
                )
                .await?;
                assert_user_delete_cascades_to_assignments(
                    conn,
                    stored_user.id,
                    retained_permission_id,
                )
                .await?;
                Ok(())
            })
        },
    ))
}
