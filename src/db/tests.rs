#[cfg(any(feature = "sqlite", feature = "postgres"))]
use diesel::prelude::*;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use diesel_async::AsyncConnection;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use diesel_async::RunQueryDsl;
#[cfg(feature = "sqlite")]
use rstest::{fixture, rstest};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use test_util::AnyError;

use super::*;
#[cfg(feature = "sqlite")]
use crate::schema::{file_acl::dsl as legacy_file_acl, files::dsl as legacy_files};
#[cfg(feature = "sqlite")]
use crate::{
    models::{
        FileNodeKind,
        NewBundle,
        NewCategory,
        NewFileNode,
        NewGroup,
        NewResourcePermission,
        NewUser,
        NewUserGroup,
    },
    schema::file_nodes::dsl as file_nodes,
};
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use crate::{
    models::{FileNodeKind, NewFileNode, NewGroup, NewResourcePermission, NewUser},
    schema::file_nodes::dsl as file_nodes,
};

#[cfg(feature = "sqlite")]
#[fixture]
async fn migrated_conn() -> Result<DbConnection, AnyError> {
    let mut conn = DbConnection::establish(":memory:").await?;
    apply_migrations(&mut conn, "", None).await?;
    Ok(conn)
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_and_get_user(#[future] migrated_conn: Result<DbConnection, AnyError>) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    let new_user = NewUser {
        username: "alice",
        password: "hash",
    };
    create_user(&mut conn, &new_user)
        .await
        .expect("failed to create user");
    let fetched = get_user_by_name(&mut conn, "alice")
        .await
        .expect("lookup failed")
        .expect("user not found");
    assert_eq!(fetched.username, "alice");
    assert_eq!(fetched.password, "hash");
}

// basic smoke test for migrations and insertion
#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_bundle_and_category(#[future] migrated_conn: Result<DbConnection, AnyError>) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    let bun = NewBundle {
        parent_bundle_id: None,
        name: "Bundle",
    };
    let _ = create_bundle(&mut conn, &bun)
        .await
        .expect("failed to create bundle");
    let cat = NewCategory {
        name: "General",
        bundle_id: None,
    };
    create_category(&mut conn, &cat)
        .await
        .expect("failed to create category");
    let _names = list_names_at_path(&mut conn, None)
        .await
        .expect("failed to list names");
}

#[cfg(feature = "sqlite")]
async fn seed_root_category(conn: &mut DbConnection, name: &'static str) {
    let cat = NewCategory {
        name,
        bundle_id: None,
    };
    create_category(conn, &cat)
        .await
        .expect("failed to seed category");
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn seed_download_permission(conn: &mut DbConnection) -> Result<i32, AnyError> {
    seed_permission(conn, &download_file_permission())
        .await
        .map_err(anyhow::Error::from)
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn create_test_user(conn: &mut DbConnection, username: &str) -> Result<i32, AnyError> {
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

#[cfg(any(feature = "sqlite", feature = "postgres"))]
struct RootFileNodeSpec<'a> {
    name: &'a str,
    object_key: &'a str,
    size: i64,
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn create_root_file_node_for_owner(
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

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn resource_permission_count(conn: &mut DbConnection) -> Result<i64, AnyError> {
    use crate::schema::resource_permissions::dsl as resource_permissions;

    resource_permissions::resource_permissions
        .count()
        .get_result::<i64>(conn)
        .await
        .map_err(anyhow::Error::from)
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_list_names_invalid_path(#[future] migrated_conn: Result<DbConnection, AnyError>) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    // Ensure we have at least one bundle to differentiate root vs invalid lookups.
    let bun = NewBundle {
        parent_bundle_id: None,
        name: "RootBundle",
    };
    create_bundle(&mut conn, &bun)
        .await
        .expect("failed to create bundle");
    let err = list_names_at_path(&mut conn, Some("/missing"))
        .await
        .expect_err("expected invalid path error");
    assert!(matches!(err, PathLookupError::InvalidPath));
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_root_article_round_trip(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    seed_root_category(&mut conn, "General").await;
    let title = "Root Article".to_string();
    let data = "Hello, world!".to_string();
    let params = CreateRootArticleParams {
        title: &title,
        flags: 0,
        data_flavor: "text/plain",
        data: &data,
    };
    let article_id = create_root_article(&mut conn, "/General", params)
        .await
        .expect("failed to create article");
    let fetched = get_article(&mut conn, "/General", article_id)
        .await
        .expect("lookup failed")
        .expect("article missing");
    assert_eq!(fetched.id, article_id);
    assert_eq!(fetched.title, title);
    let titles = list_article_titles(&mut conn, "/General")
        .await
        .expect("failed to list titles");
    assert_eq!(titles, vec![title]);
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_create_root_article_invalid_path(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    let params = CreateRootArticleParams {
        title: "Ghost",
        flags: 0,
        data_flavor: "text/plain",
        data: "ghost",
    };
    let err = create_root_article(&mut conn, "/missing", params)
        .await
        .expect_err("expected invalid path failure");
    assert!(matches!(err, PathLookupError::InvalidPath));
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_file_node_acl_flow(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    let user = NewUser {
        username: "carol",
        password: "hash",
    };
    create_user(&mut conn, &user)
        .await
        .expect("failed to create user");
    let carol = get_user_by_name(&mut conn, "carol")
        .await
        .expect("lookup failed")
        .expect("user missing");

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
    let file_id = create_file_node(&mut conn, &file)
        .await
        .expect("failed to create file node");
    let permission_id = seed_download_permission(&mut conn).await?;

    let acl = NewResourcePermission {
        resource_type: "file_node",
        resource_id: file_id,
        principal_type: "user",
        principal_id: carol.id,
        permission_id,
    };
    assert!(
        grant_resource_permission(&mut conn, &acl)
            .await
            .expect("failed to add acl")
    );
    // second insert is a no-op
    assert!(
        !grant_resource_permission(&mut conn, &acl)
            .await
            .expect("idempotent acl add")
    );

    let files = list_visible_root_file_nodes_for_user(&mut conn, carol.id)
        .await
        .expect("failed to list files");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "report.txt");
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_resolve_file_node_path_and_alias(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    let user = NewUser {
        username: "dora",
        password: "hash",
    };
    create_user(&mut conn, &user)
        .await
        .expect("failed to create user");
    let dora = get_user_by_name(&mut conn, "dora")
        .await
        .expect("lookup failed")
        .expect("user missing");

    let folder_id = create_file_node(
        &mut conn,
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
    .await
    .expect("failed to create folder");

    let file_id = create_file_node(
        &mut conn,
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
    .await
    .expect("failed to create file");

    let alias_id = create_file_node(
        &mut conn,
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
    .await
    .expect("failed to create alias");

    let resolved = resolve_file_node_path(&mut conn, "/Docs/guide.txt")
        .await
        .expect("path lookup should succeed")
        .expect("path should resolve");
    assert_eq!(resolved.id, file_id);

    let children = list_child_file_nodes(&mut conn, Some(folder_id))
        .await
        .expect("child listing should succeed");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "guide.txt");

    let target = resolve_alias_target(&mut conn, alias_id)
        .await
        .expect("alias lookup should succeed")
        .expect("alias should resolve");
    assert_eq!(target.id, file_id);
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_group_acl_visibility(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    create_user(
        &mut conn,
        &NewUser {
            username: "erin",
            password: "hash",
        },
    )
    .await
    .expect("failed to create user");
    let erin = get_user_by_name(&mut conn, "erin")
        .await
        .expect("lookup failed")
        .expect("user missing");

    let group_id = create_group(&mut conn, &NewGroup { name: "reviewers" })
        .await
        .expect("failed to create group");
    assert!(
        add_user_to_group(
            &mut conn,
            &NewUserGroup {
                user_id: erin.id,
                group_id,
            },
        )
        .await
        .expect("failed to add membership")
    );

    let node_id = create_file_node(
        &mut conn,
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
    .await
    .expect("failed to create file");
    let permission_id = seed_download_permission(&mut conn).await?;

    assert!(
        grant_resource_permission(
            &mut conn,
            &NewResourcePermission {
                resource_type: "file_node",
                resource_id: node_id,
                principal_type: "group",
                principal_id: group_id,
                permission_id,
            },
        )
        .await
        .expect("failed to grant group acl")
    );

    let visible = list_visible_root_file_nodes_for_user(&mut conn, erin.id)
        .await
        .expect("visibility query should succeed");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "shared.txt");
    Ok(())
}

pub(super) async fn test_file_nodes_reject_self_parent_body(
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

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_file_nodes_reject_self_parent(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    test_file_nodes_reject_self_parent_body(&mut conn, "CHECK constraint failed")
        .await
        .expect("self-parent guard should reject recursive parent links");
}

#[cfg(feature = "postgres")]
async fn with_embedded_pg<F>(db_name: &str, f: F) -> Result<(), test_util::AnyError>
where
    F: for<'conn> FnOnce(
        &'conn mut DbConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), test_util::AnyError>> + 'conn>,
    >,
{
    use postgresql_embedded::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().await.map_err(anyhow::Error::from)?;
    pg.start().await.map_err(anyhow::Error::from)?;
    pg.create_database(db_name)
        .await
        .map_err(anyhow::Error::from)?;
    let url = pg.settings().url(db_name);
    run_migrations(&url, None)
        .await
        .map_err(anyhow::Error::from)?;
    let mut conn = diesel_async::AsyncPgConnection::establish(&url)
        .await
        .map_err(anyhow::Error::from)?;
    let result = f(&mut conn).await;
    pg.stop().await.map_err(anyhow::Error::from)?;
    result
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_nodes_reject_self_parent() {
    with_embedded_pg("self_parent", |conn| {
        Box::pin(test_file_nodes_reject_self_parent_body(conn, "check"))
    })
    .await
    .expect("self-parent guard should reject recursive parent links");
}

pub(super) async fn test_file_nodes_reject_invalid_basenames_body(
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

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_file_nodes_reject_invalid_basenames(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    test_file_nodes_reject_invalid_basenames_body(&mut conn, "CHECK constraint failed")
        .await
        .expect("basename guard should reject empty and slash-delimited names");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_file_nodes_reject_invalid_basenames() {
    with_embedded_pg("invalid_basenames", |conn| {
        Box::pin(test_file_nodes_reject_invalid_basenames_body(conn, "check"))
    })
    .await
    .expect("basename guard should reject empty and slash-delimited names");
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

pub(super) async fn test_resource_permissions_cleanup_on_principal_delete_body(
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

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_resource_permissions_cleanup_on_principal_delete(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    test_resource_permissions_cleanup_on_principal_delete_body(&mut conn)
        .await
        .expect("principal deletes should clean up ACL rows");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resource_permissions_cleanup_on_principal_delete() {
    with_embedded_pg("cleanup_principal_delete", |conn| {
        Box::pin(test_resource_permissions_cleanup_on_principal_delete_body(
            conn,
        ))
    })
    .await
    .expect("principal deletes should clean up ACL rows");
}

pub(super) async fn test_resource_permissions_reject_unknown_principal_body(
    conn: &mut DbConnection,
) -> Result<(), AnyError> {
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

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_resource_permissions_reject_unknown_principal(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    test_resource_permissions_reject_unknown_principal_body(&mut conn)
        .await
        .expect("unknown principals should be rejected");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[serial_test::file_serial(postgres_embedded_setup)]
async fn test_resource_permissions_reject_unknown_principal() {
    with_embedded_pg("unknown_principal", |conn| {
        Box::pin(test_resource_permissions_reject_unknown_principal_body(
            conn,
        ))
    })
    .await
    .expect("unknown principals should be rejected");
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_legacy_file_acl_visibility_fallback(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    create_user(
        &mut conn,
        &NewUser {
            username: "frank",
            password: "hash",
        },
    )
    .await
    .expect("failed to create user");
    let frank = get_user_by_name(&mut conn, "frank")
        .await
        .expect("lookup failed")
        .expect("user missing");

    diesel::insert_into(legacy_files::files)
        .values((
            legacy_files::name.eq("legacy.txt"),
            legacy_files::object_key.eq("objects/legacy.txt"),
            legacy_files::size.eq(99_i64),
        ))
        .execute(&mut conn)
        .await
        .expect("failed to create legacy file");
    let file_id = legacy_files::files
        .filter(legacy_files::name.eq("legacy.txt"))
        .select(legacy_files::id)
        .first::<i32>(&mut conn)
        .await
        .expect("legacy file id");

    diesel::insert_into(legacy_file_acl::file_acl)
        .values((
            legacy_file_acl::file_id.eq(file_id),
            legacy_file_acl::user_id.eq(frank.id),
        ))
        .execute(&mut conn)
        .await
        .expect("failed to create legacy acl");

    let visible = list_visible_root_file_nodes_for_user(&mut conn, frank.id)
        .await
        .expect("visibility query should succeed");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "legacy.txt");
    assert_eq!(visible[0].kind, "file");
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_visible_root_files_merge_legacy_and_file_node_sources(
    #[future] migrated_conn: Result<DbConnection, AnyError>,
) -> Result<(), AnyError> {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    create_user(
        &mut conn,
        &NewUser {
            username: "merge-user",
            password: "hash",
        },
    )
    .await?;
    let merge_user = get_user_by_name(&mut conn, "merge-user")
        .await?
        .ok_or_else(|| anyhow::anyhow!("merge-user missing"))?;

    let permission_id = seed_download_permission(&mut conn).await?;
    let node_id = create_file_node(
        &mut conn,
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
        &mut conn,
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
        .execute(&mut conn)
        .await?;
    let legacy_id = legacy_files::files
        .filter(legacy_files::name.eq("legacy.txt"))
        .select(legacy_files::id)
        .first::<i32>(&mut conn)
        .await?;
    diesel::insert_into(legacy_file_acl::file_acl)
        .values((
            legacy_file_acl::file_id.eq(legacy_id),
            legacy_file_acl::user_id.eq(merge_user.id),
        ))
        .execute(&mut conn)
        .await?;

    let visible = list_visible_root_file_nodes_for_user(&mut conn, merge_user.id).await?;
    let visible_names = visible
        .into_iter()
        .map(|node| node.name)
        .collect::<Vec<_>>();
    assert_eq!(visible_names, vec!["legacy.txt", "modern.txt"]);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_establish_pool_returns_pool() {
    let pool = establish_pool(":memory:")
        .await
        .expect("pool creation failed");
    pool.get().await.expect("pool should yield connection");
}

#[cfg(feature = "sqlite")]
#[rstest]
#[tokio::test]
async fn test_audit_features(#[future] migrated_conn: Result<DbConnection, AnyError>) {
    let mut conn = migrated_conn
        .await
        .expect("failed to create migrated test database");
    audit_sqlite_features(&mut conn)
        .await
        .expect("sqlite feature audit failed");
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires embedded PostgreSQL server"]
async fn test_audit_postgres() {
    use postgresql_embedded::PostgreSQL;

    let mut pg = PostgreSQL::default();
    pg.setup().await.expect("failed to set up postgres");
    pg.start().await.expect("failed to start postgres");
    pg.create_database("test")
        .await
        .expect("failed to create db");
    let url = pg.settings().url("test");
    let mut conn = diesel_async::AsyncPgConnection::establish(&url)
        .await
        .expect("failed to connect to postgres");
    audit_postgres_features(&mut conn)
        .await
        .expect("postgres feature audit failed");
    pg.stop().await.expect("failed to stop postgres");
}
