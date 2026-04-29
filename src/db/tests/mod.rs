//! `SQLite` and `Postgres` database test entry points.
//!
//! This module keeps the public test surface for database behaviour while
//! gating backend-specific cases with `#[cfg(feature = "sqlite")]` or
//! `#[cfg(feature = "postgres")]`. Shared file-node scenarios and the
//! embedded-Postgres harness live in `file_node_tests` so heavier,
//! platform-specific setup stays isolated from the main test list.

#[cfg(feature = "sqlite")]
use diesel_async::AsyncConnection;
#[cfg(feature = "sqlite")]
use rstest::{fixture, rstest};
#[cfg(feature = "sqlite")]
use test_util::AnyError;

#[cfg(any(feature = "sqlite", feature = "postgres"))]
mod file_node_tests;
#[cfg(feature = "sqlite")]
mod permission_tests;
#[cfg(feature = "postgres")]
mod postgres_file_node_tests;
#[cfg(feature = "sqlite")]
mod sqlite_file_node_tests;

#[cfg(feature = "sqlite")]
use super::*;
#[cfg(feature = "sqlite")]
use crate::models::{NewBundle, NewCategory, NewUser};

/// Rstest fixture that provides a migrated in-memory `SQLite` `DbConnection`.
///
/// Creates a `:memory:` `SQLite` database, runs all pending migrations via
/// [`apply_migrations`], and returns the live connection. Intended for use as a
/// `#[future]` argument in async rstest cases that require a fully migrated
/// database.
///
/// # Errors
///
/// Returns an error if the in-memory connection cannot be established or if
/// any migration fails.
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
async fn seed_root_category(conn: &mut DbConnection, name: &'static str) -> Result<(), AnyError> {
    let cat = NewCategory {
        name,
        bundle_id: None,
    };
    create_category(conn, &cat).await?;
    Ok(())
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
    seed_root_category(&mut conn, "General")
        .await
        .expect("failed to seed category");
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
