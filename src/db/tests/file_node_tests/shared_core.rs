//! Core backend-agnostic file-node scenario bodies.

use test_util::AnyError;

use super::seed_download_permission;
use crate::{
    db::{
        DbConnection,
        add_user_to_group,
        create_file_node,
        create_group,
        create_user,
        get_user_by_name,
        grant_resource_permission,
        list_child_file_nodes,
        list_visible_root_file_nodes_for_user,
        resolve_alias_target,
        resolve_file_node_path,
    },
    models::{FileNodeKind, NewFileNode, NewGroup, NewResourcePermission, NewUser, NewUserGroup},
};

/// Verify the full direct-user ACL flow: create, check no visibility, grant,
/// check idempotency, then assert visibility.
///
/// # Errors
///
/// Propagates any database error encountered during the scenario.
#[expect(
    clippy::cognitive_complexity,
    reason = "scenario-style test body is clearer kept as a linear flow"
)]
pub(crate) async fn file_node_acl_flow_body(conn: &mut DbConnection) -> Result<(), AnyError> {
    let user = NewUser {
        username: "carol",
        password: "hash",
    };
    create_user(conn, &user).await?;
    let carol = get_user_by_name(conn, "carol")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user missing"))?;

    let file = NewFileNode {
        kind: FileNodeKind::File.as_str(),
        name: "report.txt",
        parent_id: None,
        alias_target_id: None,
        object_key: Some("objects/report.txt"),
        size: Some(42),
        comment: None,
        is_dropbox: false,
        creator_id: carol.id,
    };
    let file_id = create_file_node(conn, &file).await?;
    let files_before_grant = list_visible_root_file_nodes_for_user(conn, carol.id).await?;
    assert_eq!(files_before_grant.len(), 0);

    let permission_id = seed_download_permission(conn).await?;

    let acl = NewResourcePermission {
        resource_type: "file_node",
        resource_id: file_id,
        principal_type: "user",
        principal_id: carol.id,
        permission_id,
    };
    assert!(grant_resource_permission(conn, &acl).await?);
    assert!(!grant_resource_permission(conn, &acl).await?);

    let files = list_visible_root_file_nodes_for_user(conn, carol.id).await?;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "report.txt");
    Ok(())
}

/// Verify that `resolve_file_node_path`, `list_child_file_nodes`, and
/// `resolve_alias_target` work correctly over a two-level folder/file/alias
/// hierarchy.
///
/// # Errors
///
/// Propagates any database error encountered during the scenario.
#[expect(
    clippy::cognitive_complexity,
    reason = "scenario-style test body is clearer kept as a linear flow"
)]
pub(crate) async fn resolve_file_node_path_and_alias_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    let user = NewUser {
        username: "dora",
        password: "hash",
    };
    create_user(conn, &user).await?;
    let dora = get_user_by_name(conn, "dora")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user missing"))?;

    let folder_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::Folder.as_str(),
            name: "Docs",
            parent_id: None,
            alias_target_id: None,
            object_key: None,
            size: None,
            comment: Some("folder"),
            is_dropbox: false,
            creator_id: dora.id,
        },
    )
    .await?;

    let file_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "guide.txt",
            parent_id: Some(folder_id),
            alias_target_id: None,
            object_key: Some("objects/guide.txt"),
            size: Some(7),
            comment: None,
            is_dropbox: false,
            creator_id: dora.id,
        },
    )
    .await?;

    let alias_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::Alias.as_str(),
            name: "guide-alias",
            parent_id: None,
            alias_target_id: Some(file_id),
            object_key: None,
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id: dora.id,
        },
    )
    .await?;

    let resolved = resolve_file_node_path(conn, "/Docs/guide.txt")
        .await?
        .ok_or_else(|| anyhow::anyhow!("path should resolve"))?;
    assert_eq!(resolved.id, file_id);

    let children = list_child_file_nodes(conn, Some(folder_id)).await?;
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "guide.txt");

    let target = resolve_alias_target(conn, alias_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("alias should resolve"))?;
    assert_eq!(target.id, file_id);
    Ok(())
}

/// Verify that a group-scoped `resource_permissions` grant makes a root file
/// node visible to group members.
///
/// # Errors
///
/// Propagates any database error encountered during the scenario.
#[expect(
    clippy::cognitive_complexity,
    reason = "scenario-style test body is clearer kept as a linear flow"
)]
pub(crate) async fn group_acl_visibility_body(conn: &mut DbConnection) -> Result<(), AnyError> {
    create_user(
        conn,
        &NewUser {
            username: "erin",
            password: "hash",
        },
    )
    .await?;
    let erin = get_user_by_name(conn, "erin")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user missing"))?;

    let group_id = create_group(conn, &NewGroup { name: "reviewers" }).await?;
    assert!(
        add_user_to_group(
            conn,
            &NewUserGroup {
                user_id: erin.id,
                group_id,
            },
        )
        .await?
    );

    let node_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "shared.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("objects/shared.txt"),
            size: Some(11),
            comment: None,
            is_dropbox: false,
            creator_id: erin.id,
        },
    )
    .await?;
    let visible_before_grant = list_visible_root_file_nodes_for_user(conn, erin.id).await?;
    assert_eq!(visible_before_grant.len(), 0);

    let permission_id = seed_download_permission(conn).await?;

    assert!(
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
        .await?
    );

    let visible = list_visible_root_file_nodes_for_user(conn, erin.id).await?;
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "shared.txt");
    Ok(())
}

/// Verify that revoking a `resource_permissions` row removes root visibility.
///
/// # Errors
///
/// Propagates any database error encountered during the scenario.
pub(crate) async fn grant_revocation_removes_visibility_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;

    use crate::schema::resource_permissions::dsl as rp;

    let user_id = create_user_id(conn, "revoke-user").await?;
    let permission_id = seed_download_permission(conn).await?;
    let node_id = create_visible_root_file(conn, user_id, "revoke-me.txt").await?;

    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "user",
            principal_id: user_id,
            permission_id,
        },
    )
    .await?;

    anyhow::ensure!(
        visible_root_contains(conn, user_id, "revoke-me.txt").await?,
        "node should be visible after grant"
    );

    diesel::delete(
        rp::resource_permissions
            .filter(rp::resource_id.eq(node_id))
            .filter(rp::principal_id.eq(user_id)),
    )
    .execute(conn)
    .await?;

    anyhow::ensure!(
        !visible_root_contains(conn, user_id, "revoke-me.txt").await?,
        "node should not be visible after grant revocation"
    );
    Ok(())
}

/// Verify that removing a user from a group revokes group-scoped visibility.
///
/// # Errors
///
/// Propagates any database error encountered during the scenario.
pub(crate) async fn group_membership_removal_revokes_visibility_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;

    use crate::schema::user_groups::dsl as ug;

    let user_id = create_user_id(conn, "group-remove-user").await?;
    let permission_id = seed_download_permission(conn).await?;
    let group_id = create_group_with_member(conn, user_id).await?;
    let node_id = create_visible_root_file(conn, user_id, "group-file.txt").await?;

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

    anyhow::ensure!(
        visible_root_contains(conn, user_id, "group-file.txt").await?,
        "node should be visible while user is a group member"
    );

    diesel::delete(
        ug::user_groups
            .filter(ug::user_id.eq(user_id))
            .filter(ug::group_id.eq(group_id)),
    )
    .execute(conn)
    .await?;

    anyhow::ensure!(
        !visible_root_contains(conn, user_id, "group-file.txt").await?,
        "node should not be visible after user removed from group"
    );
    Ok(())
}

async fn create_user_id(conn: &mut DbConnection, name: &'static str) -> Result<i32, AnyError> {
    create_user(
        conn,
        &NewUser {
            username: name,
            password: "hash",
        },
    )
    .await?;
    get_user_by_name(conn, name)
        .await?
        .map(|user| user.id)
        .ok_or_else(|| anyhow::anyhow!("user missing"))
}

async fn create_group_with_member(conn: &mut DbConnection, user_id: i32) -> Result<i32, AnyError> {
    let group_id = create_group(
        conn,
        &NewGroup {
            name: "revoke-group",
        },
    )
    .await?;
    add_user_to_group(conn, &NewUserGroup { user_id, group_id }).await?;
    Ok(group_id)
}

async fn create_visible_root_file(
    conn: &mut DbConnection,
    creator_id: i32,
    name: &str,
) -> Result<i32, AnyError> {
    create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name,
            parent_id: None,
            alias_target_id: None,
            object_key: Some(name),
            size: Some(1),
            comment: None,
            is_dropbox: false,
            creator_id,
        },
    )
    .await
    .map_err(anyhow::Error::from)
}

async fn visible_root_contains(
    conn: &mut DbConnection,
    user_id: i32,
    name: &str,
) -> Result<bool, AnyError> {
    let visible = list_visible_root_file_nodes_for_user(conn, user_id).await?;
    Ok(visible.iter().any(|node| node.name == name))
}
