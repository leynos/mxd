//! Integration tests for the `file_nodes` repository helpers.

use anyhow::ensure;
use mxd::{
    db::{
        DbConnection,
        add_user_to_group,
        create_file_node,
        create_group,
        get_user_by_name,
        grant_resource_permission,
        list_child_file_nodes,
        list_visible_root_file_nodes_for_user,
        resolve_alias_target,
        resolve_file_node_path,
        seed_permission,
    },
    models::{
        FileNodeKind,
        NewFileNode,
        NewGroup,
        NewPermission,
        NewResourcePermission,
        NewUserGroup,
    },
};
use test_util::{AnyError, build_test_db, setup_login_db};
use tokio::runtime::Runtime;

async fn fetch_alice(conn: &mut DbConnection) -> Result<mxd::models::User, AnyError> {
    get_user_by_name(conn, "alice")
        .await?
        .ok_or_else(|| anyhow::anyhow!("alice fixture user missing"))
}

const fn file_node(kind: FileNodeKind, name: &str, creator_id: i32) -> NewFileNode<'_> {
    NewFileNode {
        kind: kind.as_str(),
        name,
        parent_id: None,
        alias_target_id: None,
        object_key: None,
        size: None,
        comment: None,
        is_dropbox: false,
        creator_id,
    }
}

fn with_test_db(
    f: impl FnOnce(Runtime, test_util::TestDb) -> Result<(), AnyError>,
) -> Result<(), AnyError> {
    let runtime = Runtime::new()?;
    let Some(db) = build_test_db(&runtime, setup_login_db)? else {
        return Ok(());
    };
    f(runtime, db)
}

async fn seed_docs_tree(
    conn: &mut DbConnection,
    creator_id: i32,
) -> Result<(i32, i32, i32), AnyError> {
    let mut folder = file_node(FileNodeKind::Folder, "Docs", creator_id);
    folder.comment = Some("docs");
    let folder_id = create_file_node(conn, &folder).await?;

    let mut file = file_node(FileNodeKind::File, "guide.txt", creator_id);
    file.parent_id = Some(folder_id);
    file.object_key = Some("objects/guide.txt");
    file.size = Some(123);
    let file_id = create_file_node(conn, &file).await?;

    let mut alias = file_node(FileNodeKind::Alias, "guide-link", creator_id);
    alias.alias_target_id = Some(file_id);
    let alias_id = create_file_node(conn, &alias).await?;

    Ok((folder_id, file_id, alias_id))
}

#[test]
fn resolves_paths_and_aliases() -> Result<(), AnyError> {
    with_test_db(|runtime, db| {
        runtime.block_on(async move {
            let pool = db.pool();
            let mut conn = pool.get().await?;
            let alice = fetch_alice(&mut conn).await?;
            let (folder_id, file_id, alias_id) = seed_docs_tree(&mut conn, alice.id).await?;

            let resolved = resolve_file_node_path(&mut conn, "/Docs/guide.txt")
                .await?
                .ok_or_else(|| anyhow::anyhow!("expected file path to resolve"))?;
            ensure!(
                resolved.id == file_id,
                "resolved path should match inserted file"
            );

            let children = list_child_file_nodes(&mut conn, Some(folder_id)).await?;
            ensure!(
                children.len() == 1,
                "folder should contain exactly one child"
            );
            let child = children
                .first()
                .ok_or_else(|| anyhow::anyhow!("folder child missing after insert"))?;
            ensure!(
                child.name == "guide.txt",
                "folder child should keep its basename"
            );

            let alias_target = resolve_alias_target(&mut conn, alias_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("expected alias target to resolve"))?;
            ensure!(
                alias_target.id == file_id,
                "alias should resolve to the file target"
            );
            Ok(())
        })
    })
}

#[test]
fn group_acl_grants_root_visibility() -> Result<(), AnyError> {
    with_test_db(|runtime, db| {
        runtime.block_on(async move {
            let pool = db.pool();
            let mut conn = pool.get().await?;
            let alice = get_user_by_name(&mut conn, "alice")
                .await?
                .ok_or_else(|| anyhow::anyhow!("alice fixture user missing"))?;

            let permission_id = seed_permission(
                &mut conn,
                &NewPermission {
                    code: 2,
                    name: "download_file",
                    description: "List or download a file node",
                },
            )
            .await?;

            let group_id = create_group(&mut conn, &NewGroup { name: "everyone" }).await?;
            let _added = add_user_to_group(
                &mut conn,
                &NewUserGroup {
                    user_id: alice.id,
                    group_id,
                },
            )
            .await?;

            let node_id = create_file_node(
                &mut conn,
                &NewFileNode {
                    kind: FileNodeKind::File.as_str(),
                    name: "shared.txt",
                    parent_id: None,
                    alias_target_id: None,
                    object_key: Some("objects/shared.txt"),
                    size: Some(64),
                    comment: None,
                    is_dropbox: false,
                    creator_id: alice.id,
                },
            )
            .await?;

            let _granted = grant_resource_permission(
                &mut conn,
                &NewResourcePermission {
                    resource_type: "file_node",
                    resource_id: node_id,
                    principal_type: "group",
                    principal_id: group_id,
                    permission_id,
                },
            )
            .await?;

            let visible = list_visible_root_file_nodes_for_user(&mut conn, alice.id).await?;
            ensure!(
                visible.len() == 1,
                "group acl should expose one visible file node"
            );
            let node = visible
                .first()
                .ok_or_else(|| anyhow::anyhow!("visible node missing after acl grant"))?;
            ensure!(
                node.name == "shared.txt",
                "group acl should expose the expected file"
            );
            Ok(())
        })
    })
}
