//! Unit tests for connection-level session handling.

use super::*;
use crate::wireframe::test_helpers::dummy_pool;

#[tokio::test]
async fn context_carries_shared_argon2_reference() {
    let pool = dummy_pool();
    let argon2 = Arc::new(Argon2::default());
    let peer: SocketAddr = "127.0.0.1:9001".parse().expect("loopback address");

    let ctx = Context::new(peer, pool, Arc::clone(&argon2));

    assert!(Arc::ptr_eq(&ctx.argon2, &argon2));
    assert_eq!(Arc::strong_count(&argon2), 2);
    assert_eq!(ctx.peer, peer);
}

#[tokio::test]
async fn multiple_contexts_share_single_argon2_instance() {
    let pool = dummy_pool();
    let argon2 = Arc::new(Argon2::default());

    let ctx_a = Context::new(
        "127.0.0.1:9002".parse().expect("loopback"),
        pool.clone(),
        Arc::clone(&argon2),
    );
    let ctx_b = Context::new(
        "127.0.0.1:9003".parse().expect("loopback"),
        pool,
        Arc::clone(&argon2),
    );

    assert!(Arc::ptr_eq(&ctx_a.argon2, &argon2));
    assert!(Arc::ptr_eq(&ctx_b.argon2, &argon2));
    assert_eq!(Arc::strong_count(&argon2), 3);

    drop(ctx_a);
    assert_eq!(Arc::strong_count(&argon2), 2);
    drop(ctx_b);
    assert_eq!(Arc::strong_count(&argon2), 1);
}

#[test]
fn session_default_is_unauthenticated() {
    let session = Session::default();
    assert!(!session.is_authenticated());
    assert!(session.privileges.is_empty());
    assert_eq!(session.phase, SessionPhase::Unauthenticated);
    assert!(session.display_name.is_empty());
    assert_eq!(session.icon_id, 0);
    assert!(session.connection_flags.is_empty());
    assert!(session.auto_response.is_none());
}

#[test]
fn session_is_authenticated_with_user_id() {
    let session = Session {
        user_id: Some(42),
        ..Default::default()
    };
    assert!(session.is_authenticated());
    assert!(!session.is_online());
}

#[test]
fn session_has_privilege_returns_true_when_present() {
    let session = Session {
        user_id: Some(1),
        privileges: Privileges::DOWNLOAD_FILE,
        ..Default::default()
    };
    assert!(session.has_privilege(Privileges::DOWNLOAD_FILE));
}

#[test]
fn session_has_privilege_returns_false_when_absent() {
    let session = Session {
        user_id: Some(1),
        privileges: Privileges::DOWNLOAD_FILE,
        ..Default::default()
    };
    assert!(!session.has_privilege(Privileges::UPLOAD_FILE));
}

#[test]
fn session_require_privilege_fails_when_unauthenticated() {
    let session = Session::default();
    let result = session.require_privilege(Privileges::DOWNLOAD_FILE);
    assert_eq!(result, Err(PrivilegeError::NotAuthenticated));
}

#[test]
fn session_require_privilege_fails_when_missing_privilege() {
    let session = Session {
        user_id: Some(1),
        privileges: Privileges::READ_CHAT,
        ..Default::default()
    };
    let result = session.require_privilege(Privileges::DOWNLOAD_FILE);
    assert_eq!(
        result,
        Err(PrivilegeError::InsufficientPrivileges(
            Privileges::DOWNLOAD_FILE
        ))
    );
}

#[test]
fn session_require_privilege_succeeds_when_present() {
    let session = Session {
        user_id: Some(1),
        privileges: Privileges::DOWNLOAD_FILE | Privileges::READ_CHAT,
        ..Default::default()
    };
    let result = session.require_privilege(Privileges::DOWNLOAD_FILE);
    assert!(result.is_ok());
}

#[test]
fn session_require_authenticated_fails_when_unauthenticated() {
    let session = Session::default();
    let result = session.require_authenticated();
    assert_eq!(result, Err(PrivilegeError::NotAuthenticated));
}

#[test]
fn session_require_authenticated_succeeds_when_logged_in() {
    let session = Session {
        user_id: Some(1),
        ..Default::default()
    };
    let result = session.require_authenticated();
    assert!(result.is_ok());
}

#[test]
fn privilege_error_display_not_authenticated() {
    let err = PrivilegeError::NotAuthenticated;
    assert_eq!(err.to_string(), "authentication required");
}

#[test]
fn privilege_error_display_insufficient_privileges() {
    let err = PrivilegeError::InsufficientPrivileges(Privileges::DOWNLOAD_FILE);
    assert!(err.to_string().contains("insufficient privileges"));
}
