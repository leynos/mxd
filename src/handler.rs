//! Connection-level request processing.
//!
//! The handler owns per-client [`Session`] state and dispatches incoming
//! transactions to [`Command`] processors.
use std::{
    error::Error,
    fmt,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use argon2::Argon2;

use crate::{
    commands::{Command, CommandError, ProcessContext},
    connection_flags::ConnectionFlags,
    db::DbPool,
    presence::{PresenceRegistry, PresenceSnapshot, SessionPhase},
    privileges::Privileges,
    server::outbound::OutboundConnectionId,
    transaction::{Transaction, parse_transaction},
};

static NEXT_LEGACY_PRESENCE_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

/// Per-connection context used by `handle_request`.
#[derive(Clone)]
pub struct Context {
    /// Remote peer socket address.
    pub peer: SocketAddr,
    /// Database connection pool.
    pub pool: DbPool,
    /// Shared Argon2 instance for password hashing.
    pub argon2: Arc<Argon2<'static>>,
    /// Shared presence registry for legacy request processing.
    pub presence: Arc<PresenceRegistry>,
    /// Adapter-owned identifier used when publishing this connection's presence.
    pub presence_connection_id: OutboundConnectionId,
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
    /// Connection lifecycle state for protocol visibility.
    pub phase: SessionPhase,
    /// Session-visible nickname.
    pub display_name: String,
    /// Session-visible icon identifier.
    pub icon_id: u16,
    /// Connection-level preference flags (refuse messages, auto-response, etc.).
    ///
    /// Set during login/agreement and can be updated via `SetClientUserInfo`.
    pub connection_flags: ConnectionFlags,
    /// Automatic response text associated with the current session.
    pub auto_response: Option<String>,
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

    /// Check whether the session is fully online.
    #[must_use]
    pub const fn is_online(&self) -> bool { matches!(self.phase, SessionPhase::Online) }

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

    /// Update the authenticated account details after a successful login.
    pub fn apply_login(&mut self, user_id: i32, username: &str, privileges: Privileges) {
        self.user_id = Some(user_id);
        self.privileges = privileges;
        username.clone_into(&mut self.display_name);
        self.icon_id = 0;
        self.auto_response = None;
        self.connection_flags = ConnectionFlags::default();
        self.phase = if self.requires_agreement() {
            SessionPhase::PendingAgreement
        } else {
            SessionPhase::Online
        };
    }

    /// Return whether the account must complete agreement before going online.
    #[must_use]
    pub const fn requires_agreement(&self) -> bool {
        !self.privileges.contains(Privileges::NO_AGREEMENT)
    }

    /// Return whether the session should appear in the public user list.
    #[must_use]
    pub const fn shows_in_user_list(&self) -> bool {
        self.is_online() && self.privileges.contains(Privileges::SHOW_IN_LIST)
    }

    /// Return the packed user-list colour/status flags for this session.
    #[must_use]
    pub fn presence_flags(&self) -> u16 { if self.is_presence_admin() { 2 } else { 0 } }

    /// Build a public presence snapshot when the session is online and visible.
    #[must_use]
    pub fn presence_snapshot(
        &self,
        connection_id: OutboundConnectionId,
    ) -> Option<PresenceSnapshot> {
        let user_id = self.user_id?;
        if !self.shows_in_user_list() {
            return None;
        }
        Some(PresenceSnapshot {
            connection_id,
            user_id,
            display_name: self.display_name.clone(),
            icon_id: self.icon_id,
            status_flags: self.presence_flags(),
        })
    }

    fn is_presence_admin(&self) -> bool {
        self.privileges.intersects(
            Privileges::CREATE_USER
                | Privileges::DELETE_USER
                | Privileges::OPEN_USER
                | Privileges::MODIFY_USER
                | Privileges::DISCONNECT_USER
                | Privileges::BROADCAST,
        )
    }
}

impl Context {
    /// Create a new connection context.
    #[must_use]
    pub fn new(peer: SocketAddr, pool: DbPool, argon2: Arc<Argon2<'static>>) -> Self {
        Self {
            peer,
            pool,
            argon2,
            presence: Arc::new(PresenceRegistry::default()),
            presence_connection_id: next_legacy_presence_connection_id(),
        }
    }

    /// Create a new connection context with shared presence.
    #[must_use]
    pub fn with_presence(
        peer: SocketAddr,
        pool: DbPool,
        argon2: Arc<Argon2<'static>>,
        presence: Arc<PresenceRegistry>,
    ) -> Self {
        Self {
            peer,
            pool,
            argon2,
            presence,
            presence_connection_id: next_legacy_presence_connection_id(),
        }
    }
}

fn next_legacy_presence_connection_id() -> OutboundConnectionId {
    let id = NEXT_LEGACY_PRESENCE_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
    OutboundConnectionId::new(id)
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
    cmd.process(ProcessContext {
        peer: ctx.peer,
        pool: ctx.pool.clone(),
        session,
        presence: ctx.presence.as_ref(),
        presence_connection_id: Some(ctx.presence_connection_id),
    })
    .await
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
