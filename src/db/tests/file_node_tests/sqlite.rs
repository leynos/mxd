//! SQLite-only file-node test bodies.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
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
    schema::{file_acl::dsl as legacy_file_acl, files::dsl as legacy_files},
};

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

#[expect(
    clippy::cognitive_complexity,
    reason = "scenario-style test body is clearer kept as a linear flow"
)]
pub(crate) async fn legacy_file_acl_visibility_fallback_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
    create_user(
        conn,
        &NewUser {
            username: "frank",
            password: "hash",
        },
    )
    .await?;
    let frank = get_user_by_name(conn, "frank")
        .await?
        .ok_or_else(|| anyhow::anyhow!("user missing"))?;

    diesel::insert_into(legacy_files::files)
        .values((
            legacy_files::name.eq("legacy.txt"),
            legacy_files::object_key.eq("objects/legacy.txt"),
            legacy_files::size.eq(99_i64),
        ))
        .execute(conn)
        .await?;
    let file_id = legacy_files::files
        .filter(legacy_files::name.eq("legacy.txt"))
        .select(legacy_files::id)
        .first::<i32>(conn)
        .await?;

    diesel::insert_into(legacy_file_acl::file_acl)
        .values((
            legacy_file_acl::file_id.eq(file_id),
            legacy_file_acl::user_id.eq(frank.id),
        ))
        .execute(conn)
        .await?;

    let visible = list_visible_root_file_nodes_for_user(conn, frank.id).await?;
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "legacy.txt");
    assert_eq!(visible[0].kind, "file");
    Ok(())
}

#[expect(
    clippy::cognitive_complexity,
    reason = "scenario-style test body is clearer kept as a linear flow"
)]
pub(crate) async fn visible_root_files_merge_body(conn: &mut DbConnection) -> Result<(), AnyError> {
    create_user(
        conn,
        &NewUser {
            username: "merge-user",
            password: "hash",
        },
    )
    .await?;
    let merge_user = get_user_by_name(conn, "merge-user")
        .await?
        .ok_or_else(|| anyhow::anyhow!("merge-user missing"))?;

    let permission_id = seed_download_permission(conn).await?;
    let node_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: "modern.txt",
            parent_id: None,
            alias_target_id: None,
            object_key: Some("objects/modern.txt"),
            size: Some(11),
            comment: None,
            is_dropbox: false,
            creator_id: merge_user.id,
        },
    )
    .await?;
    grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "user",
            principal_id: merge_user.id,
            permission_id,
        },
    )
    .await?;

    diesel::insert_into(legacy_files::files)
        .values((
            legacy_files::name.eq("legacy.txt"),
            legacy_files::object_key.eq("objects/legacy.txt"),
            legacy_files::size.eq(7_i64),
        ))
        .execute(conn)
        .await?;
    let legacy_id = legacy_files::files
        .filter(legacy_files::name.eq("legacy.txt"))
        .select(legacy_files::id)
        .first::<i32>(conn)
        .await?;
    diesel::insert_into(legacy_file_acl::file_acl)
        .values((
            legacy_file_acl::file_id.eq(legacy_id),
            legacy_file_acl::user_id.eq(merge_user.id),
        ))
        .execute(conn)
        .await?;

    let visible = list_visible_root_file_nodes_for_user(conn, merge_user.id).await?;
    let visible_names = visible
        .into_iter()
        .map(|node| node.name)
        .collect::<Vec<_>>();
    assert_eq!(visible_names, vec!["legacy.txt", "modern.txt"]);
    Ok(())
}
