//! Integration tests for file-node repository helpers.
//!
//! These tests exercise the additive `file_nodes` schema and recursive
//! traversal against the active backend selected by the current test target.
#![expect(clippy::expect_used, reason = "test assertions")]

use mxd::{
    db::{
        FILE_NODE_RESOURCE_TYPE,
        USER_PRINCIPAL_TYPE,
        add_resource_permission,
        create_file_node,
        create_user,
        get_root_file_node,
        get_user_by_name,
        list_descendant_file_nodes,
        list_permitted_child_file_nodes_for_user,
    },
    models::{NewFileNode, NewResourcePermission, NewUser},
    privileges::Privileges,
};
use rstest::rstest;
use test_util::{AnyError, build_test_db_async, setup_login_db};

async fn seed_user(pool: &mxd::db::DbPool, username: &'static str) -> Result<i32, AnyError> {
    let mut conn = pool.get().await?;
    if let Some(user) = get_user_by_name(&mut conn, username).await? {
        return Ok(user.id);
    }
    create_user(
        &mut conn,
        &NewUser {
            username,
            password: "hash",
        },
    )
    .await?;
    Ok(get_user_by_name(&mut conn, username)
        .await?
        .expect("seeded user should exist")
        .id)
}

#[rstest]
#[tokio::test]
async fn recursive_descendants_work_on_active_backend() -> Result<(), AnyError> {
    let Some(test_db) = build_test_db_async(setup_login_db).await? else {
        return Ok(());
    };
    let pool = test_db.pool();
    let alice_id = seed_user(&pool, "alice").await?;
    let mut conn = pool.get().await?;
    let root = get_root_file_node(&mut conn).await?;
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
            created_by: Some(alice_id),
        },
    )
    .await?;
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
            comment: Some("notes"),
            is_dropbox: false,
            created_by: Some(alice_id),
        },
    )
    .await?;

    let descendants = list_descendant_file_nodes(&mut conn, root.id).await?;

    assert_eq!(
        descendants
            .into_iter()
            .map(|node| (node.name, node.depth))
            .collect::<Vec<_>>(),
        vec![
            (String::new(), 0),
            ("docs".to_owned(), 1),
            ("readme.txt".to_owned(), 2),
        ]
    );
    Ok(())
}

#[rstest]
#[tokio::test]
async fn explicit_resource_grants_filter_children_on_active_backend() -> Result<(), AnyError> {
    let Some(test_db) = build_test_db_async(setup_login_db).await? else {
        return Ok(());
    };
    let pool = test_db.pool();
    let alice_id = seed_user(&pool, "alice").await?;
    let bob_id = seed_user(&pool, "bob").await?;
    let mut conn = pool.get().await?;
    let root = get_root_file_node(&mut conn).await?;
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
    .await?;
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
    .await?;
    add_resource_permission(
        &mut conn,
        &NewResourcePermission {
            resource_type: FILE_NODE_RESOURCE_TYPE,
            resource_id: shared_id,
            principal_type: USER_PRINCIPAL_TYPE,
            principal_id: bob_id,
            privileges: i64::try_from(Privileges::DOWNLOAD_FILE.bits())
                .expect("download privilege bitmask fits within i64"),
        },
    )
    .await?;

    let permitted = list_permitted_child_file_nodes_for_user(&mut conn, root.id, bob_id).await?;

    assert_eq!(
        permitted
            .into_iter()
            .map(|node| node.name)
            .collect::<Vec<_>>(),
        vec!["shared.txt".to_owned()]
    );
    Ok(())
}
