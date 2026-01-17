//! Connection-level request processing.
//!
//! The handler owns per-client [`Session`] state and dispatches incoming
//! transactions to [`Command`] processors. Each connection runs in its own
//! asynchronous task.
use std::{error::Error, fmt, net::SocketAddr, sync::Arc};

use argon2::Argon2;

use crate::{
    commands::{Command, CommandError},
    connection_flags::ConnectionFlags,
    db::DbPool,
    privileges::Privileges,
    transaction::{Transaction, parse_transaction},
};

/// Per-connection context used by `handle_request`.
#[derive(Clone)]
pub struct Context {
    /// Remote peer socket address.
    pub peer: SocketAddr,
    /// Database connection pool.
    pub pool: DbPool,
    /// Shared Argon2 instance for password hashing.
    pub argon2: Arc<Argon2<'static>>,
}

/// Session state for a single connection.
///
/// Tracks authentication status, privileges, and user preferences. The session
/// is shared across all transaction handlers for a connection via
/// `Arc<tokio::sync::Mutex<Session>>` in the wireframe middleware.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// Authenticated user identifier, if logged in.
    pub user_id: Option<i32>,
    /// User access privileges from the Hotline protocol.
    ///
    /// Populated on successful login; empty until authenticated.
    pub privileges: Privileges,
    /// Connection-level preference flags (refuse messages, auto-response, etc.).
    ///
    /// Set during login/agreement and can be updated via `SetClientUserInfo`.
    pub connection_flags: ConnectionFlags,
}

/// Error returned when a privilege check fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivilegeError {
    /// The user is not authenticated (no login has occurred).
    NotAuthenticated,
    /// The user lacks the required privilege for this operation.
    InsufficientPrivileges(Privileges),
}

impl fmt::Display for PrivilegeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAuthenticated => write!(f, "authentication required"),
            Self::InsufficientPrivileges(p) => {
                write!(f, "insufficient privileges: {}", format_privileges(*p))
            }
        }
    }
}

impl Error for PrivilegeError {}

fn format_privileges(privileges: Privileges) -> String {
    let names: Vec<String> = privileges
        .iter_names()
        .map(|(name, _)| name.to_ascii_lowercase().replace('_', " "))
        .collect();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

impl Session {
    /// Check whether the session is authenticated.
    #[must_use]
    pub const fn is_authenticated(&self) -> bool { self.user_id.is_some() }

    /// Check whether the session has a specific privilege.
    ///
    /// Returns `false` if the user is not authenticated or lacks the privilege.
    #[must_use]
    pub const fn has_privilege(&self, priv_required: Privileges) -> bool {
        self.user_id.is_some() && self.privileges.contains(priv_required)
    }

    /// Require that the session is authenticated and has the specified privilege.
    ///
    /// # Errors
    ///
    /// Returns [`PrivilegeError::NotAuthenticated`] if not logged in, or
    /// [`PrivilegeError::InsufficientPrivileges`] if the privilege is missing.
    pub const fn require_privilege(&self, priv_required: Privileges) -> Result<(), PrivilegeError> {
        match (
            self.user_id.is_some(),
            self.privileges.contains(priv_required),
        ) {
            (false, _) => Err(PrivilegeError::NotAuthenticated),
            (true, false) => Err(PrivilegeError::InsufficientPrivileges(priv_required)),
            (true, true) => Ok(()),
        }
    }

    /// Require that the session is authenticated (any privilege level).
    ///
    /// # Errors
    ///
    /// Returns [`PrivilegeError::NotAuthenticated`] if not logged in.
    pub const fn require_authenticated(&self) -> Result<(), PrivilegeError> {
        match self.user_id {
            Some(_) => Ok(()),
            None => Err(PrivilegeError::NotAuthenticated),
        }
    }
}

impl Context {
    /// Create a new connection context.
    #[expect(
        clippy::missing_const_for_fn,
        reason = "const fn with Arc may not be portable across Rust versions"
    )]
    #[must_use]
    pub fn new(peer: SocketAddr, pool: DbPool, argon2: Arc<Argon2<'static>>) -> Self {
        Self { peer, pool, argon2 }
    }
}

/// Parse and handle a single request frame without performing network I/O.
///
/// # Errors
/// Returns an error if the transaction cannot be parsed or processed.
#[must_use = "handle the result"]
pub async fn handle_request(
    ctx: &Context,
    session: &mut Session,
    frame: &[u8],
) -> Result<Transaction, CommandError> {
    let tx = parse_transaction(frame)?;
    let cmd = Command::from_transaction(tx)?;
    cmd.process(ctx.peer, ctx.pool.clone(), session).await
}

#[cfg(test)]
mod tests {
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
        assert!(session.connection_flags.is_empty());
    }

    #[test]
    fn session_is_authenticated_with_user_id() {
        let session = Session {
            user_id: Some(42),
            ..Default::default()
        };
        assert!(session.is_authenticated());
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
}
