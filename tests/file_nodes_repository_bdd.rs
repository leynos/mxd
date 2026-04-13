//! Behaviour-driven tests for file-node repository operations.
#![expect(clippy::expect_used, reason = "test assertions")]

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

use mxd::{
    db::{
        FILE_NODE_RESOURCE_TYPE,
        USER_PRINCIPAL_TYPE,
        add_resource_permission,
        create_file_node,
        create_user,
        get_root_file_node,
        get_user_by_name,
        list_permitted_child_file_nodes_for_user,
    },
    models::{NewFileNode, NewResourcePermission, NewUser},
    privileges::Privileges,
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use test_util::{SetupFn, TestDb, build_test_db, setup_login_db};
use tokio::runtime::Runtime;

struct FileNodeWorld {
    runtime: Runtime,
    db: RefCell<Option<TestDb>>,
    users: RefCell<HashMap<String, i32>>,
    root_id: Cell<Option<i32>>,
    last_error: RefCell<Option<String>>,
    permitted_names: RefCell<Vec<String>>,
    skipped: Cell<bool>,
}

impl FileNodeWorld {
    fn new() -> Self {
        Self {
            runtime: Runtime::new().expect("runtime creation should succeed"),
            db: RefCell::new(None),
            users: RefCell::new(HashMap::new()),
            root_id: Cell::new(None),
            last_error: RefCell::new(None),
            permitted_names: RefCell::new(Vec::new()),
            skipped: Cell::new(false),
        }
    }

    const fn is_skipped(&self) -> bool { self.skipped.get() }

    fn with_pool<T>(&self, f: impl FnOnce(mxd::db::DbPool) -> T) -> T {
        let pool = self
            .db
            .borrow()
            .as_ref()
            .expect("database should be initialised")
            .pool();
        f(pool)
    }

    fn block_on_with_pool<T, F, Fut>(&self, f: F) -> T
    where
        F: FnOnce(mxd::db::DbPool) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.runtime.block_on(self.with_pool(f))
    }
}

#[fixture]
fn world() -> FileNodeWorld {
    let world = FileNodeWorld::new();
    debug_assert!(!world.is_skipped(), "world starts active");
    world
}

#[given("a migrated file-node repository")]
fn given_repository(world: &FileNodeWorld) {
    let db = match build_test_db(&world.runtime, setup_login_db as SetupFn) {
        Ok(Some(db)) => db,
        Ok(None) => {
            world.skipped.set(true);
            return;
        }
        Err(err) => panic!("failed to build test database: {err}"),
    };
    let root_id = world.runtime.block_on({
        let pool = db.pool();
        async move {
            let mut conn = pool
                .get()
                .await
                .expect("pool connection should be available");
            get_root_file_node(&mut conn)
                .await
                .expect("root node should exist")
                .id
        }
    });
    world.root_id.set(Some(root_id));
    world.db.replace(Some(db));
}

#[given("a user \"{username}\" exists")]
fn given_user(world: &FileNodeWorld, username: String) {
    if world.is_skipped() {
        return;
    }
    let username_key = username.clone();
    let user_id = world.block_on_with_pool(|pool| async move {
        let mut conn = pool
            .get()
            .await
            .expect("pool connection should be available");
        if let Some(user) = get_user_by_name(&mut conn, &username)
            .await
            .expect("user lookup should succeed")
        {
            return user.id;
        }
        create_user(
            &mut conn,
            &NewUser {
                username: &username,
                password: "hash",
            },
        )
        .await
        .expect("user creation should succeed");
        get_user_by_name(&mut conn, &username)
            .await
            .expect("user lookup should succeed")
            .expect("created user should exist")
            .id
    });
    world.users.borrow_mut().insert(username_key, user_id);
}

#[when("I create the root child folder \"{name}\" as \"{username}\"")]
fn when_create_root_child_folder(world: &FileNodeWorld, name: String, username: String) {
    if world.is_skipped() {
        return;
    }
    let user_id = *world
        .users
        .borrow()
        .get(&username)
        .expect("user should exist in the world");
    let root_id = world.root_id.get().expect("root id should be stored");
    world.last_error.borrow_mut().take();
    world.block_on_with_pool(|pool| async move {
        let mut conn = pool
            .get()
            .await
            .expect("pool connection should be available");
        let result = create_file_node(
            &mut conn,
            &NewFileNode {
                is_root: false,
                node_type: "folder",
                name: &name,
                parent_id: Some(root_id),
                alias_target_id: None,
                object_key: None,
                size: 0,
                comment: None,
                is_dropbox: false,
                created_by: Some(user_id),
            },
        )
        .await;
        if let Err(err) = result {
            world.last_error.borrow_mut().replace(err.to_string());
        }
    });
}

#[when("I try to create the root child folder \"{name}\" as \"{username}\"")]
fn when_try_duplicate_folder(world: &FileNodeWorld, name: String, username: String) {
    when_create_root_child_folder(world, name, username);
}

#[given("a root file \"{name}\" created by \"{username}\"")]
fn given_root_file(world: &FileNodeWorld, name: String, username: String) {
    if world.is_skipped() {
        return;
    }
    let user_id = *world
        .users
        .borrow()
        .get(&username)
        .expect("user should exist in the world");
    let root_id = world.root_id.get().expect("root id should be stored");
    world.block_on_with_pool(|pool| async move {
        let mut conn = pool
            .get()
            .await
            .expect("pool connection should be available");
        create_file_node(
            &mut conn,
            &NewFileNode {
                is_root: false,
                node_type: "file",
                name: &name,
                parent_id: Some(root_id),
                alias_target_id: None,
                object_key: Some(&format!("objects/{name}")),
                size: 1,
                comment: None,
                is_dropbox: false,
                created_by: Some(user_id),
            },
        )
        .await
        .expect("file insert should succeed");
    });
}

#[given("\"{username}\" has download access to \"{name}\"")]
fn given_download_access(world: &FileNodeWorld, username: String, name: String) {
    if world.is_skipped() {
        return;
    }
    let user_id = *world
        .users
        .borrow()
        .get(&username)
        .expect("user should exist in the world");
    let root_id = world.root_id.get().expect("root id should be stored");
    world.block_on_with_pool(|pool| async move {
        let mut conn = pool
            .get()
            .await
            .expect("pool connection should be available");
        let shared = mxd::db::find_child_file_node(&mut conn, root_id, &name)
            .await
            .expect("file lookup should succeed")
            .expect("file should exist");
        add_resource_permission(
            &mut conn,
            &NewResourcePermission {
                resource_type: FILE_NODE_RESOURCE_TYPE,
                resource_id: shared.id,
                principal_type: USER_PRINCIPAL_TYPE,
                principal_id: user_id,
                privileges: i64::try_from(Privileges::DOWNLOAD_FILE.bits())
                    .expect("download privilege bitmask fits within i64"),
            },
        )
        .await
        .expect("resource permission insert should succeed");
    });
}

#[when("I list root children permitted for \"{username}\"")]
fn when_list_permitted_children(world: &FileNodeWorld, username: String) {
    if world.is_skipped() {
        return;
    }
    let user_id = *world
        .users
        .borrow()
        .get(&username)
        .expect("user should exist in the world");
    let root_id = world.root_id.get().expect("root id should be stored");
    let names = world.block_on_with_pool(|pool| async move {
        let mut conn = pool
            .get()
            .await
            .expect("pool connection should be available");
        list_permitted_child_file_nodes_for_user(&mut conn, root_id, user_id)
            .await
            .expect("permitted child listing should succeed")
            .into_iter()
            .map(|node| node.name)
            .collect::<Vec<_>>()
    });
    world.permitted_names.replace(names);
}

#[then("the duplicate insert is rejected")]
fn then_duplicate_rejected(world: &FileNodeWorld) {
    if world.is_skipped() {
        return;
    }
    let error = world
        .last_error
        .borrow()
        .clone()
        .expect("duplicate insert should record an error");
    assert!(
        error.contains("UNIQUE") || error.contains("duplicate key"),
        "expected uniqueness failure, got {error}"
    );
}

#[then("the permitted child names equal \"{name}\"")]
fn then_permitted_names(world: &FileNodeWorld, name: String) {
    if world.is_skipped() {
        return;
    }
    assert_eq!(world.permitted_names.borrow().as_slice(), &[name]);
}

#[scenario(path = "tests/features/file_nodes_repository.feature", index = 0)]
fn duplicate_top_level_names_are_rejected(#[from(world)] world: FileNodeWorld) {
    given_repository(&world);
    given_user(&world, "alice".to_owned());
    when_create_root_child_folder(&world, "docs".to_owned(), "alice".to_owned());
    when_try_duplicate_folder(&world, "docs".to_owned(), "alice".to_owned());
    then_duplicate_rejected(&world);
}

#[scenario(path = "tests/features/file_nodes_repository.feature", index = 1)]
fn explicit_resource_grants_filter_visible_children(#[from(world)] world: FileNodeWorld) {
    given_repository(&world);
    given_user(&world, "alice".to_owned());
    given_user(&world, "bob".to_owned());
    given_root_file(&world, "shared.txt".to_owned(), "alice".to_owned());
    given_root_file(&world, "private.txt".to_owned(), "alice".to_owned());
    given_download_access(&world, "bob".to_owned(), "shared.txt".to_owned());
    when_list_permitted_children(&world, "bob".to_owned());
    then_permitted_names(&world, "shared.txt".to_owned());
}
