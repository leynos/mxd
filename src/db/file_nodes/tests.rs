#![cfg(feature = "sqlite")]

use diesel::{ConnectionError, result::DatabaseErrorKind};
use diesel_async::AsyncConnection;
use rstest::rstest;

use super::*;
use crate::{
    db::{DbConnection, apply_migrations, create_user, get_user_by_name},
    models::{NewResourcePermission, NewUser},
    privileges::Privileges,
};

async fn migrated_conn() -> Result<DbConnection, ConnectionError> {
    let mut conn = DbConnection::establish(":memory:").await?;
    apply_migrations(&mut conn, "")
        .await
        .expect("failed to apply migrations");
    Ok(conn)
}

async fn seed_user(conn: &mut DbConnection, username: &'static str) -> i32 {
    create_user(
        conn,
        &NewUser {
            username,
            password: "hash",
        },
    )
    .await
    .expect("failed to create user");
    get_user_by_name(conn, username)
        .await
        .expect("failed to load seeded user")
        .expect("seeded user missing")
        .id
}

#[rstest]
#[tokio::test]
async fn root_file_node_is_seeded() {
    let mut conn = migrated_conn().await.expect("connection");
    let root = get_root_file_node(&mut conn).await.expect("root node");

    assert!(root.is_root);
    assert_eq!(root.node_type, "folder");
    assert_eq!(root.parent_id, None);
}

#[rstest]
#[tokio::test]
async fn duplicate_top_level_names_are_rejected() {
    let mut conn = migrated_conn().await.expect("connection");
    let root = get_root_file_node(&mut conn).await.expect("root node");
    let user_id = seed_user(&mut conn, "alice").await;
    let folder = NewFileNode {
        is_root: false,
        node_type: "folder",
        name: "docs",
        parent_id: Some(root.id),
        alias_target_id: None,
        object_key: None,
        size: 0,
        comment: None,
        is_dropbox: false,
        created_by: Some(user_id),
    };

    create_file_node(&mut conn, &folder)
        .await
        .expect("first insert should succeed");
    let err = create_file_node(&mut conn, &folder)
        .await
        .expect_err("duplicate name should be rejected");

    assert!(matches!(
        err,
        diesel::result::Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)
    ));
}

#[rstest]
#[tokio::test]
async fn alias_requires_target() {
    let mut conn = migrated_conn().await.expect("connection");
    let root = get_root_file_node(&mut conn).await.expect("root node");
    let user_id = seed_user(&mut conn, "alice").await;
    let alias = NewFileNode {
        is_root: false,
        node_type: "alias",
        name: "link",
        parent_id: Some(root.id),
        alias_target_id: None,
        object_key: None,
        size: 0,
        comment: None,
        is_dropbox: false,
        created_by: Some(user_id),
    };

    let err = create_file_node(&mut conn, &alias)
        .await
        .expect_err("alias without target should fail");

    assert!(matches!(
        err,
        diesel::result::Error::DatabaseError(DatabaseErrorKind::CheckViolation, _)
    ));
}

#[rstest]
#[tokio::test]
async fn descendant_listing_uses_recursive_tree() {
    let mut conn = migrated_conn().await.expect("connection");
    let root = get_root_file_node(&mut conn).await.expect("root node");
    let user_id = seed_user(&mut conn, "alice").await;
    let folder_id = create_file_node(
        &mut conn,
        &NewFileNode {
            is_root: false,
            node_type: "folder",
            name: "docs",
            parent_id: Some(root.id),
            alias_target_id: None,
            object_key: None,
            size: 0,
            comment: None,
            is_dropbox: false,
            created_by: Some(user_id),
        },
    )
    .await
    .expect("folder insert");
    create_file_node(
        &mut conn,
        &NewFileNode {
            is_root: false,
            node_type: "file",
            name: "readme.txt",
            parent_id: Some(folder_id),
            alias_target_id: None,
            object_key: Some("objects/readme.txt"),
            size: 12,
            comment: None,
            is_dropbox: false,
            created_by: Some(user_id),
        },
    )
    .await
    .expect("file insert");

    let descendants = list_descendant_file_nodes(&mut conn, root.id)
        .await
        .expect("descendants");

    assert_eq!(descendants[0].name, "");
    assert_eq!(descendants[1].name, "docs");
    assert_eq!(descendants[1].depth, 1);
    assert_eq!(descendants[2].name, "readme.txt");
    assert_eq!(descendants[2].depth, 2);
}

#[rstest]
#[tokio::test]
async fn permitted_child_listing_filters_by_explicit_grant() {
    let mut conn = migrated_conn().await.expect("connection");
    let root = get_root_file_node(&mut conn).await.expect("root node");
    let alice_id = seed_user(&mut conn, "alice").await;
    let bob_id = seed_user(&mut conn, "bob").await;
    let shared_id = create_file_node(
        &mut conn,
        &NewFileNode {
            is_root: false,
            node_type: "file",
            name: "shared.txt",
            parent_id: Some(root.id),
            alias_target_id: None,
            object_key: Some("objects/shared.txt"),
            size: 42,
            comment: None,
            is_dropbox: false,
            created_by: Some(alice_id),
        },
    )
    .await
    .expect("shared file insert");
    create_file_node(
        &mut conn,
        &NewFileNode {
            is_root: false,
            node_type: "file",
            name: "private.txt",
            parent_id: Some(root.id),
            alias_target_id: None,
            object_key: Some("objects/private.txt"),
            size: 7,
            comment: None,
            is_dropbox: false,
            created_by: Some(alice_id),
        },
    )
    .await
    .expect("private file insert");
    let privileges = i64::try_from(Privileges::DOWNLOAD_FILE.bits())
        .expect("download privilege bitmask fits within i64");
    let inserted = add_resource_permission(
        &mut conn,
        &NewResourcePermission {
            resource_type: FILE_NODE_RESOURCE_TYPE,
            resource_id: shared_id,
            principal_type: USER_PRINCIPAL_TYPE,
            principal_id: bob_id,
            privileges,
        },
    )
    .await
    .expect("permission insert");

    assert!(inserted);

    let permitted = list_permitted_child_file_nodes_for_user(&mut conn, root.id, bob_id)
        .await
        .expect("permitted child listing");

    assert_eq!(permitted.len(), 1);
    assert_eq!(permitted[0].name, "shared.txt");
}
