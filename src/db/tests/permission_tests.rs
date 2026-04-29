//! Permission model and cascade tests for the `SQLite` backend.

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
) -> Result<PermissionFixture, AnyError> {
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

async fn seed_user_permission(conn: &mut DbConnection) -> Result<(i32, i32), AnyError> {
    let user = NewUser {
        username: "dana",
        password: "hash",
    };
    create_user(conn, &user).await?;
    let stored_user = get_user_by_name(conn, "dana")
        .await?
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
    #[future] permission_fixture: Result<PermissionFixture, AnyError>,
) -> Result<(), AnyError> {
    let PermissionFixture {
        mut conn,
        user_id,
        permission_id,
    } = permission_fixture.await?;

    let permission = permissions::permissions
        .filter(permissions::id.eq(permission_id))
        .first::<Permission>(&mut conn)
        .await?;
    assert_eq!(permission.code, 34);
    assert_eq!(permission.name, "News Create Category");
    assert_eq!(
        permission.description,
        "News category creation permission"
    );

    let assigned = user_permissions::user_permissions
        .first::<UserPermission>(&mut conn)
        .await?;
    assert_eq!(assigned.user_id, user_id);
    assert_eq!(assigned.permission_id, permission_id);
    Ok(())
}

#[rstest]
#[case::delete_user(DeleteTarget::User)]
#[case::delete_permission(DeleteTarget::Permission)]
#[tokio::test]
async fn test_user_permission_cascades(
    #[future] permission_fixture: Result<PermissionFixture, AnyError>,
    #[case] delete_target: DeleteTarget,
) -> Result<(), AnyError> {
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
    assert!(assignments.is_empty());
    Ok(())
}

async fn assert_permission_remains(
    conn: &mut DbConnection,
    permission_id: i32,
) -> Result<(), AnyError> {
    let permission = permissions::permissions
        .filter(permissions::id.eq(permission_id))
        .first::<Permission>(conn)
        .await?;
    assert_eq!(permission.code, 34);
    Ok(())
}

async fn assert_user_remains(conn: &mut DbConnection, user_id: i32) -> Result<(), AnyError> {
    let user = get_user_by_name(conn, "dana")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user should remain after permission deletion"))?;
    assert_eq!(user.id, user_id);
    Ok(())
}
