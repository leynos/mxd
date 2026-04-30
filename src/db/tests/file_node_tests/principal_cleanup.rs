//! Principal cleanup scenario bodies for file-node ACL tests.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use test_util::AnyError;

use super::{
    RootFileNodeSpec,
    create_root_file_node_for_owner,
    create_test_user,
    seed_download_permission,
};
use crate::{
    db::{DbConnection, create_group, grant_resource_permission},
    models::{NewGroup, NewResourcePermission},
};

async fn resource_permission_count(conn: &mut DbConnection) -> Result<i64, AnyError> {
    use crate::schema::resource_permissions::dsl as resource_permissions;

    resource_permissions::resource_permissions
        .count()
        .get_result::<i64>(conn)
        .await
        .map_err(anyhow::Error::from)
}

async fn grant_cleanup_permissions(conn: &mut DbConnection) -> Result<(i32, i32), AnyError> {
    let owner_id = create_test_user(conn, "owner").await?;
    let grantee_id = create_test_user(conn, "grantee").await?;
    let group_id = create_group(conn, &NewGroup { name: "cleanup" }).await?;
    let permission_id = seed_download_permission(conn).await?;
    let node_id = create_root_file_node_for_owner(
        conn,
        owner_id,
        RootFileNodeSpec {
            name: "cleanup.txt",
            object_key: "objects/cleanup.txt",
            size: 9,
        },
    )
    .await?;

    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "user",
            principal_id: grantee_id,
            permission_id,
        },
    )
    .await?;
    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "group",
            principal_id: group_id,
            permission_id,
        },
    )
    .await?;

    Ok((grantee_id, group_id))
}

async fn delete_cleanup_principals(
    conn: &mut DbConnection,
    grantee_id: i32,
    group_id: i32,
) -> Result<(), AnyError> {
    use crate::schema::{groups::dsl as groups_dsl, users::dsl as users_dsl};

    diesel::delete(users_dsl::users.filter(users_dsl::id.eq(grantee_id)))
        .execute(conn)
        .await?;
    diesel::delete(groups_dsl::groups.filter(groups_dsl::id.eq(group_id)))
        .execute(conn)
        .await?;

    Ok(())
}

/// Verify that deleting a user or group cascades to `resource_permissions`.
///
/// # Errors
///
/// Returns an error if the ACL count was unexpected or a database operation
/// failed.
pub(crate) async fn cleanup_on_principal_delete_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    let initial_count = resource_permission_count(conn).await?;
    anyhow::ensure!(
        initial_count == 0,
        "cleanup test expected empty ACL table at start, found {initial_count} rows"
    );

    let (grantee_id, group_id) = grant_cleanup_permissions(conn).await?;
    let granted_count = resource_permission_count(conn).await?;
    anyhow::ensure!(
        granted_count == 2,
        "cleanup test expected 2 ACL rows after grants, found {granted_count}"
    );

    delete_cleanup_principals(conn, grantee_id, group_id).await?;

    let remaining_count = resource_permission_count(conn).await?;
    anyhow::ensure!(
        remaining_count == 0,
        "cleanup triggers left {remaining_count} ACL rows after deleting principals"
    );
    Ok(())
}
