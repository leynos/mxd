//! Permission cascade tests for the `PostgreSQL` backend.

use std::future::Future;

use diesel::prelude::*;
use diesel_async::{AsyncConnection, RunQueryDsl};
use test_util::postgres::{PostgresTestDb, PostgresTestDbError};

use super::{DbConnection, apply_migrations};
use crate::{
    db::{create_user, get_user_by_name},
    models::{NewPermission, NewUser, NewUserPermission, Permission, UserPermission},
    schema::{
        permissions::dsl as permissions,
        user_permissions::dsl as user_permissions,
        users::dsl as users,
    },
};

type TestResult<T> = anyhow::Result<T>;

fn start_postgres_db() -> TestResult<Option<PostgresTestDb>> {
    match PostgresTestDb::new() {
        Ok(db) => Ok(Some(db)),
        Err(PostgresTestDbError::Unavailable(_)) => {
            tracing::warn!("SKIP-TEST-CLUSTER: PostgreSQL unavailable");
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

fn with_postgres_test_db<F, Fut>(test: F) -> TestResult<()>
where
    F: FnOnce(String) -> Fut + Send + 'static,
    Fut: Future<Output = TestResult<()>> + Send + 'static,
{
    let Some(db) = start_postgres_db()? else {
        return Ok(());
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move { test(db.url.to_string()).await })
}

#[test]
fn test_user_permission_cascades() -> TestResult<()> {
    with_postgres_test_db(|url| async move {
        let mut conn = DbConnection::establish(&url).await?;
        apply_migrations(&mut conn, &url).await?;

        let user = NewUser {
            username: "postgres-dana",
            password: "hash",
        };
        create_user(&mut conn, &user).await?;
        let stored_user = get_user_by_name(&mut conn, "postgres-dana")
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(|| anyhow::anyhow!("postgres permission test user missing"))?;

        let permission = NewPermission {
            code: 3401,
            name: "News Create Category",
            scope: "bundle",
        };
        diesel::insert_into(permissions::permissions)
            .values(&permission)
            .execute(&mut conn)
            .await?;
        let permission_id = permissions::permissions
            .filter(permissions::code.eq(3401))
            .select(permissions::id)
            .first::<i32>(&mut conn)
            .await?;

        let user_permission = NewUserPermission {
            user_id: stored_user.id,
            permission_id,
        };
        diesel::insert_into(user_permissions::user_permissions)
            .values(&user_permission)
            .execute(&mut conn)
            .await?;

        diesel::delete(users::users.filter(users::id.eq(stored_user.id)))
            .execute(&mut conn)
            .await?;

        let assignments = user_permissions::user_permissions
            .load::<UserPermission>(&mut conn)
            .await?;
        anyhow::ensure!(assignments.is_empty(), "cascade left assignments behind");

        let stored_permission = permissions::permissions
            .filter(permissions::id.eq(permission_id))
            .first::<Permission>(&mut conn)
            .await?;
        anyhow::ensure!(
            stored_permission.code == 3401,
            "permission changed after user deletion"
        );
        Ok(())
    })
}
