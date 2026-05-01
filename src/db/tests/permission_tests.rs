//! Permission model and cascade tests for the `SQLite` backend.
//!
//! Validates round-trip persistence of the `Permission` model and the
//! `user_permissions` join table, and verifies that cascade deletion removes
//! join rows when either the user or the permission is deleted.  Tests run
//! against an in-memory `SQLite` database seeded by the standard migrated
//! fixture.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use rstest::{fixture, rstest};
use test_util::AnyError;

use super::{DbConnection, migrated_conn};
use crate::{
    db::{create_user, get_user_by_name},
    models::{NewPermission, NewUser, NewUserPermission, Permission, UserPermission},
    schema::{
        permissions::dsl as permissions,
        user_permissions::dsl as user_permissions,
        users::dsl as users,
    },
};

struct PermissionFixture {
    conn: DbConnection,
    user_id: i32,
    permission_id: i32,
}

enum DeleteTarget {
    User,
    Permission,
}

#[fixture]
async fn permission_fixture(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<PermissionFixture, anyhow::Error> {
    let mut conn = migrated_conn.await?;
    diesel::sql_query("PRAGMA foreign_keys = ON")
        .execute(&mut conn)
        .await?;

    let (user_id, permission_id) = seed_user_permission(&mut conn).await?;
    let user_permission = NewUserPermission {
        user_id,
        permission_id,
    };
    diesel::insert_into(user_permissions::user_permissions)
        .values(&user_permission)
        .execute(&mut conn)
        .await?;

    Ok(PermissionFixture {
        conn,
        user_id,
        permission_id,
    })
}

async fn seed_user_permission(conn: &mut DbConnection) -> Result<(i32, i32), anyhow::Error> {
    let user = NewUser {
        username: "dana",
        password: "hash",
    };
    create_user(conn, &user).await?;
    let stored_user = get_user_by_name(conn, "dana")
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(|| anyhow::anyhow!("permission test user missing"))?;

    let permission = NewPermission {
        code: 34,
        name: "News Create Category",
        description: "News category creation permission",
    };
    diesel::insert_into(permissions::permissions)
        .values(&permission)
        .execute(conn)
        .await?;
    let permission_id = permissions::permissions
        .filter(permissions::code.eq(34))
        .select(permissions::id)
        .first::<i32>(conn)
        .await?;

    Ok((stored_user.id, permission_id))
}

#[rstest]
#[tokio::test]
async fn test_permission_model_round_trip(
    #[future] permission_fixture: Result<PermissionFixture, anyhow::Error>,
) -> Result<(), anyhow::Error> {
    let PermissionFixture {
        mut conn,
        user_id,
        permission_id,
    } = permission_fixture.await?;

    let permission = permissions::permissions
        .filter(permissions::id.eq(permission_id))
        .first::<Permission>(&mut conn)
        .await?;
    anyhow::ensure!(permission.code == 34, "unexpected permission code");
    anyhow::ensure!(
        permission.name == "News Create Category",
        "unexpected permission name"
    );
    anyhow::ensure!(
        permission.description == "News category creation permission",
        "unexpected permission description"
    );

    let assigned = user_permissions::user_permissions
        .first::<UserPermission>(&mut conn)
        .await?;
    anyhow::ensure!(assigned.user_id == user_id, "unexpected assigned user");
    anyhow::ensure!(
        assigned.permission_id == permission_id,
        "unexpected assigned permission"
    );
    Ok(())
}

#[rstest]
#[case::delete_user(DeleteTarget::User)]
#[case::delete_permission(DeleteTarget::Permission)]
#[tokio::test]
async fn test_user_permission_cascades(
    #[future] permission_fixture: Result<PermissionFixture, anyhow::Error>,
    #[case] delete_target: DeleteTarget,
) -> Result<(), anyhow::Error> {
    let PermissionFixture {
        mut conn,
        user_id,
        permission_id,
    } = permission_fixture.await?;

    match delete_target {
        DeleteTarget::User => {
            diesel::delete(users::users.filter(users::id.eq(user_id)))
                .execute(&mut conn)
                .await?;
            assert_permission_remains(&mut conn, permission_id).await?;
        }
        DeleteTarget::Permission => {
            diesel::delete(permissions::permissions.filter(permissions::id.eq(permission_id)))
                .execute(&mut conn)
                .await?;
            assert_user_remains(&mut conn, user_id).await?;
        }
    }

    let assignments = user_permissions::user_permissions
        .load::<UserPermission>(&mut conn)
        .await?;
    anyhow::ensure!(assignments.is_empty(), "cascade left assignments behind");
    Ok(())
}

async fn assert_permission_remains(
    conn: &mut DbConnection,
    permission_id: i32,
) -> Result<(), anyhow::Error> {
    let permission = permissions::permissions
        .filter(permissions::id.eq(permission_id))
        .first::<Permission>(conn)
        .await?;
    anyhow::ensure!(
        permission.code == 34,
        "permission changed after user deletion"
    );
    Ok(())
}

async fn assert_user_remains(conn: &mut DbConnection, user_id: i32) -> Result<(), anyhow::Error> {
    let user = get_user_by_name(conn, "dana")
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(|| anyhow::anyhow!("user should remain after permission deletion"))?;
    anyhow::ensure!(
        user.id == user_id,
        "unexpected user after permission deletion"
    );
    Ok(())
}
