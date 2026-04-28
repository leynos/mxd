//! Additional backend-agnostic file-node scenario bodies.

#[cfg(feature = "sqlite")]
use rstest::rstest;
use test_util::AnyError;

#[cfg(feature = "sqlite")]
use super::super::migrated_conn;
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

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_resolve_file_node_path_returns_none_for_missing_path(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    super::resolve_file_node_path_returns_none_for_missing_path_body(&mut conn)
        .await
        .expect("missing-path should return None");
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_non_download_permission_does_not_grant_visibility(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    super::non_download_permission_does_not_grant_visibility_body(&mut conn)
        .await
        .expect("non-download permission should not grant visibility");
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_nested_child_not_visible_without_explicit_grant(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    super::nested_child_not_visible_without_explicit_grant_body(&mut conn)
        .await
        .expect("nested child should not appear without explicit grant");
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_file_node_check_kind_constraints(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn.await.expect("migrated db");
    super::file_node_check_kind_constraint_body(&mut conn)
        .await
        .expect("kind-specific CHECK constraints should be enforced");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resolve_file_node_path_returns_none_for_missing_path() {
    super::with_embedded_pg("missing_path", |conn| {
        Box::pin(super::resolve_file_node_path_returns_none_for_missing_path_body(conn))
    })
    .await
    .expect("missing-path should return None");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_non_download_permission_does_not_grant_visibility() {
    super::with_embedded_pg("non_download_permission", |conn| {
        Box::pin(super::non_download_permission_does_not_grant_visibility_body(conn))
    })
    .await
    .expect("non-download permission should not grant visibility");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_nested_child_not_visible_without_explicit_grant() {
    super::with_embedded_pg("nested_child_visibility", |conn| {
        Box::pin(super::nested_child_not_visible_without_explicit_grant_body(
            conn,
        ))
    })
    .await
    .expect("nested child should not appear without explicit grant");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_node_check_kind_constraints() {
    super::with_embedded_pg("kind_constraints", |conn| {
        Box::pin(super::file_node_check_kind_constraint_body(conn))
    })
    .await
    .expect("kind-specific CHECK constraints should be enforced");
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
    let child_visible = visible.iter().any(|n| n.name == "child.txt");
    anyhow::ensure!(
        !child_visible,
        "child file should not appear as visible root node without its own grant"
    );
    Ok(())
}

/// Verify that kind-specific `CHECK` constraints are enforced.
///
/// Attempts invalid inserts for a file without data fields, a folder with an
/// object key, and an alias without a target.
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

    assert_file_without_data_rejected(conn, user.id).await?;
    assert_folder_with_object_key_rejected(conn, user.id).await?;
    assert_alias_without_target_rejected(conn, user.id).await?;

    Ok(())
}

async fn assert_file_without_data_rejected(
    conn: &mut DbConnection,
    creator_id: i32,
) -> Result<(), AnyError> {
    let file_without_data = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "no-key.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: None,
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id,
        },
    )
    .await;
    anyhow::ensure!(
        file_without_data.is_err(),
        "file without object_key should be rejected"
    );
    Ok(())
}

async fn assert_folder_with_object_key_rejected(
    conn: &mut DbConnection,
    creator_id: i32,
) -> Result<(), AnyError> {
    let folder_with_object_key = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::Folder.as_str(),
            name: "folder-with-key",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("objects/folder.bin"),
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id,
        },
    )
    .await;
    anyhow::ensure!(
        folder_with_object_key.is_err(),
        "folder with object_key should be rejected"
    );
    Ok(())
}

async fn assert_alias_without_target_rejected(
    conn: &mut DbConnection,
    creator_id: i32,
) -> Result<(), AnyError> {
    let alias_without_target = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::Alias.as_str(),
            name: "orphan-alias",
            parent_id: None,
            alias_target_id: None,
            object_key: None,
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id,
        },
    )
    .await;
    anyhow::ensure!(
        alias_without_target.is_err(),
        "alias without alias_target_id should be rejected"
    );
    Ok(())
}
