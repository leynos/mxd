//! Constraint and rejection scenario bodies for file-node tests.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use test_util::AnyError;

use super::seed_download_permission;
use crate::{
    db::{
        DbConnection,
        create_file_node,
        create_user,
        get_user_by_name,
        grant_resource_permission,
    },
    models::{FileNodeKind, NewFileNode, NewResourcePermission, NewUser},
    schema::file_nodes::dsl as file_nodes,
};

/// Seed a test user and return its generated ID.
///
/// # Errors
///
/// Propagates user creation or lookup errors.
pub(crate) async fn create_test_user(
    conn: &mut DbConnection,
    username: &str,
) -> Result<i32, AnyError> {
    create_user(
        conn,
        &NewUser {
            username,
            password: "hash",
        },
    )
    .await?;

    get_user_by_name(conn, username)
        .await?
        .map(|user| user.id)
        .ok_or_else(|| anyhow::anyhow!("test user '{username}' missing after insert"))
}

/// Describes a root file node inserted for constraint-oriented tests.
pub(crate) struct RootFileNodeSpec<'a> {
    /// Root-level file name.
    pub(crate) name: &'a str,
    /// Object storage key.
    pub(crate) object_key: &'a str,
    /// File size in bytes.
    pub(crate) size: i64,
}

/// Create a root file node owned by `owner_id`.
///
/// # Errors
///
/// Propagates any database error from inserting the node.
pub(crate) async fn create_root_file_node_for_owner(
    conn: &mut DbConnection,
    owner_id: i32,
    spec: RootFileNodeSpec<'_>,
) -> Result<i32, AnyError> {
    create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::File.as_str(),
            name: spec.name,
            parent_id: None,
            alias_target_id: None,
            object_key: Some(spec.object_key),
            size: Some(spec.size),
            comment: None,
            is_dropbox: false,
            creator_id: owner_id,
        },
    )
    .await
    .map_err(anyhow::Error::from)
}

/// Verify that the database rejects an update that would make a node its own
/// parent.
///
/// # Errors
///
/// Returns an error if the constraint was not enforced or a database operation
/// failed unexpectedly.
pub(crate) async fn reject_self_parent_body(
    conn: &mut DbConnection,
    check_msg: &str,
) -> Result<(), AnyError> {
    let owner_id = create_test_user(conn, "selfparent").await?;

    let folder_id = create_file_node(
        conn,
        &NewFileNode {
            kind: FileNodeKind::Folder.as_str(),
            name: "Loop",
            parent_id: None,
            alias_target_id: None,
            object_key: None,
            size: None,
            comment: None,
            is_dropbox: false,
            creator_id: owner_id,
        },
    )
    .await?;

    let Err(err) = diesel::update(file_nodes::file_nodes.filter(file_nodes::id.eq(folder_id)))
        .set(file_nodes::parent_id.eq(Some(folder_id)))
        .execute(conn)
        .await
    else {
        anyhow::bail!("self-parent update should fail");
    };
    anyhow::ensure!(
        err.to_string()
            .to_lowercase()
            .contains(&check_msg.to_lowercase()),
        "self-parent update returned unexpected error: {err}"
    );
    Ok(())
}

/// Verify that the database rejects file nodes with invalid basenames.
///
/// # Errors
///
/// Returns an error if an invalid basename was accepted or a database operation
/// failed unexpectedly.
pub(crate) async fn reject_invalid_basenames_body(
    conn: &mut DbConnection,
    check_msg: &str,
) -> Result<(), AnyError> {
    let owner_id = create_test_user(conn, "basename-owner").await?;

    for (index, invalid_name) in ["", "bad/name"].into_iter().enumerate() {
        let object_key = format!("objects/invalid-name-{index}.txt");
        let Err(err) = create_file_node(
            conn,
            &NewFileNode {
                kind: FileNodeKind::File.as_str(),
                name: invalid_name,
                parent_id: None,
                alias_target_id: None,
                object_key: Some(object_key.as_str()),
                size: Some(1),
                comment: None,
                is_dropbox: false,
                creator_id: owner_id,
            },
        )
        .await
        else {
            anyhow::bail!("invalid basename '{invalid_name}' should be rejected");
        };
        anyhow::ensure!(
            err.to_string()
                .to_lowercase()
                .contains(&check_msg.to_lowercase()),
            "invalid basename '{invalid_name}' returned unexpected error: {err}"
        );
    }

    Ok(())
}

/// Verify that `grant_resource_permission` rejects an unknown principal ID.
///
/// # Errors
///
/// Returns an error if the row was accepted or a database operation failed.
pub(crate) async fn reject_unknown_principal_body(conn: &mut DbConnection) -> Result<(), AnyError> {
    let owner_id = create_test_user(conn, "principal-owner").await?;
    let permission_id = seed_download_permission(conn).await?;
    let node_id = create_root_file_node_for_owner(
        conn,
        owner_id,
        RootFileNodeSpec {
            name: "principal-check.txt",
            object_key: "objects/principal-check.txt",
            size: 5,
        },
    )
    .await?;

    let Err(err) = grant_resource_permission(
        conn,
        &NewResourcePermission {
            resource_type: "file_node",
            resource_id: node_id,
            principal_type: "user",
            principal_id: i32::MAX,
            permission_id,
        },
    )
    .await
    else {
        anyhow::bail!("unknown principal should be rejected");
    };
    anyhow::ensure!(
        err.to_string().contains("resource_permissions principal"),
        "unknown principal returned unexpected error: {err}"
    );
    Ok(())
}
