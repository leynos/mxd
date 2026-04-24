//! File-sharing-specific fixture helpers.

use std::collections::HashMap;

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use mxd::{
    db::{
        DbConnection,
        add_user_to_group,
        create_file_node,
        create_group,
        download_file_permission,
        grant_resource_permission,
        seed_permission,
    },
    models::{FileNodeKind, NewFileNode, NewGroup, NewResourcePermission, NewUserGroup},
    schema::users::dsl as users_dsl,
};

use crate::AnyError;

/// Resolve a file name to its file-node ID from the lookup map.
fn resolve_file_node_id(file_node_ids: &HashMap<String, i32>, name: &str) -> Result<i32, AnyError> {
    file_node_ids
        .get(name)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("missing file-node id for {name}"))
}

pub(crate) async fn fetch_test_user_id(conn: &mut DbConnection) -> Result<i32, AnyError> {
    users_dsl::users
        .filter(users_dsl::username.eq("alice"))
        .select(users_dsl::id)
        .first(conn)
        .await
        .map_err(Into::into)
}

pub(crate) async fn seed_download_file_permission(
    conn: &mut DbConnection,
) -> Result<i32, AnyError> {
    seed_permission(conn, &download_file_permission())
        .await
        .map_err(Into::into)
}

pub(crate) async fn ensure_everyone_group_membership(
    conn: &mut DbConnection,
    user_id: i32,
) -> Result<(), AnyError> {
    let everyone_group_id = create_group(conn, &NewGroup { name: "everyone" }).await?;
    let _group_added = add_user_to_group(
        conn,
        &NewUserGroup {
            user_id,
            group_id: everyone_group_id,
        },
    )
    .await?;
    Ok(())
}

pub(crate) async fn seed_root_file_nodes(
    conn: &mut DbConnection,
    creator_id: i32,
) -> Result<HashMap<String, i32>, AnyError> {
    let file_nodes = [
        NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "fileA.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("1"),
            size: Some(1),
            comment: None,
            is_dropbox: false,
            creator_id,
        },
        NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "fileB.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("2"),
            size: Some(1),
            comment: None,
            is_dropbox: false,
            creator_id,
        },
        NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "fileC.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("3"),
            size: Some(1),
            comment: None,
            is_dropbox: false,
            creator_id,
        },
    ];
    let mut file_node_ids = HashMap::with_capacity(file_nodes.len());
    for file_node in &file_nodes {
        let node_id = create_file_node(conn, file_node).await?;
        file_node_ids.insert(file_node.name.to_owned(), node_id);
    }
    Ok(file_node_ids)
}

pub(crate) async fn grant_fixture_download_visibility(
    conn: &mut DbConnection,
    user_id: i32,
    permission_id: i32,
    file_node_ids: &HashMap<String, i32>,
) -> Result<(), AnyError> {
    for name in ["fileA.txt", "fileC.txt"] {
        let resource_id = resolve_file_node_id(file_node_ids, name)?;
        grant_resource_permission(
            conn,
            &NewResourcePermission {
                resource_type: "file_node",
                resource_id,
                principal_type: "user",
                principal_id: user_id,
                permission_id,
            },
        )
        .await?;
    }
    Ok(())
}
