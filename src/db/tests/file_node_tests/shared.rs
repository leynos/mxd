//! Additional backend-agnostic file-node scenario bodies.

use test_util::AnyError;

use super::seed_download_permission;
use crate::{
    db::{
        DbConnection,
        create_file_node,
        create_user,
        get_user_by_name,
        grant_resource_permission,
        list_visible_root_file_nodes_for_user,
        resolve_file_node_path,
        seed_permission,
    },
    models::{FileNodeKind, NewFileNode, NewPermission, NewResourcePermission, NewUser},
};

/// Assert that `resolve_file_node_path` returns `Ok(None)` for a path that
/// does not exist in the hierarchy.
///
/// # Errors
///
/// Propagates any database error encountered during the test.
pub(crate) async fn resolve_file_node_path_returns_none_for_missing_path_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    let result = resolve_file_node_path(conn, "/does/not/exist").await?;
    anyhow::ensure!(
        result.is_none(),
        "path that was never inserted should resolve to None, got {result:?}"
    );
    Ok(())
}

/// Assert that granting a permission whose code differs from the canonical
/// `download_file` code (2) does not make a root file node visible.
///
/// # Errors
///
/// Propagates any database error encountered during the test.
pub(crate) async fn non_download_permission_does_not_grant_visibility_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    create_user(
        conn,
        &NewUser {
            username: "perm-filter-user",
            password: "hash",
        },
    )
    .await?;
    let user = get_user_by_name(conn, "perm-filter-user")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user missing"))?;
    let other_perm_id = seed_permission(
        conn,
        &NewPermission {
            code: 99,
            name: "other_permission",
            description: "A non-download permission",
        },
    )
    .await?;
    let node_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "hidden.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("objects/hidden.txt"),
            size: Some(1),
            comment: None,
            is_dropbox: false,
            creator_id: user.id,
        },
    )
    .await?;
    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "user",
            principal_id: user.id,
            permission_id: other_perm_id,
        },
    )
    .await?;
    let visible = list_visible_root_file_nodes_for_user(conn, user.id).await?;
    anyhow::ensure!(
        visible.is_empty(),
        "non-download permission should not grant root visibility, found {} nodes",
        visible.len()
    );
    Ok(())
}

/// Assert that a file node nested inside a folder is not surfaced in root
/// visibility even when a download permission exists for the parent folder.
///
/// Root visibility (`list_visible_root_file_nodes_for_user`) returns only
/// nodes that have an explicit `resource_permissions` row; folder-level grants
/// do not cascade to children.
///
/// # Errors
///
/// Propagates any database error encountered during the test.
pub(crate) async fn nested_child_not_visible_without_explicit_grant_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    create_user(
        conn,
        &NewUser {
            username: "nested-user",
            password: "hash",
        },
    )
    .await?;
    let user = get_user_by_name(conn, "nested-user")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user missing"))?;
    let folder_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::Folder.as_str(),
            name: "ParentFolder",
            parent_id: None,
            alias_target_id: None,
            object_key: None,
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id: user.id,
        },
    )
    .await?;
    create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "child.txt",
            parent_id: Some(folder_id),
            alias_target_id: None,
            object_key: Some("objects/child.txt"),
            size: Some(5),
            comment: None,
            is_dropbox: false,
            creator_id: user.id,
        },
    )
    .await?;
    let permission_id = seed_download_permission(conn).await?;
    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: folder_id,
            principal_type: "user",
            principal_id: user.id,
            permission_id,
        },
    )
    .await?;
    let visible = list_visible_root_file_nodes_for_user(conn, user.id).await?;
    anyhow::ensure!(
        !visible.iter().any(|n| n.name == "child.txt"),
        "child file should not appear as visible root node without its own grant"
    );
    Ok(())
}

/// Verify that kind-specific `CHECK` constraints are enforced.
///
/// Attempts invalid inserts for a file without data fields, a folder with an
/// object key, and an alias without a target. Each must be rejected.
///
/// # Errors
///
/// Returns an error if any constraint was not enforced or if a database
/// operation fails unexpectedly.
pub(crate) async fn file_node_check_kind_constraint_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    create_user(
        conn,
        &NewUser {
            username: "constraint-user",
            password: "hash",
        },
    )
    .await?;
    let user = get_user_by_name(conn, "constraint-user")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user missing"))?;

    assert_invalid_file_node_rejected(
        conn,
        "file without object_key should be rejected",
        NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "no-key.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: None,
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id: user.id,
        },
    )
    .await?;
    assert_invalid_file_node_rejected(
        conn,
        "folder with object_key should be rejected",
        NewFileNode {
            kind: FileNodeKind::Folder.as_str(),
            name: "folder-with-key",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("objects/folder.bin"),
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id: user.id,
        },
    )
    .await?;
    assert_invalid_file_node_rejected(
        conn,
        "alias without alias_target_id should be rejected",
        NewFileNode {
            kind: FileNodeKind::Alias.as_str(),
            name: "orphan-alias",
            parent_id: None,
            alias_target_id: None,
            object_key: None,
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id: user.id,
        },
    )
    .await
}

async fn assert_invalid_file_node_rejected(
    conn: &mut DbConnection,
    message: &'static str,
    node: NewFileNode<'_>,
) -> Result<(), AnyError> {
    let result = create_file_node(conn, &node).await;
    anyhow::ensure!(result.is_err(), message);
    Ok(())
}
