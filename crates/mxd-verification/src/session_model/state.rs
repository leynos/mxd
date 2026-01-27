//! State types for the session gating model.
//!
//! This module defines the core state structures that Stateright explores:
//! - [`ModelSession`] — Per-client authentication state and privileges
//! - [`RequestType`] — Abstract request types with privilege requirements
//! - [`ModelMessage`] — Queued request wrapper for out-of-order delivery
//! - [`Effect`] — Observable outcomes for invariant checking
//! - [`SystemState`] — Global system state containing all client sessions

use std::hash::Hash;

use super::privileges::{
    DOWNLOAD_FILE,
    GET_CLIENT_INFO,
    NEWS_POST_ARTICLE,
    NEWS_READ_ARTICLE,
    NO_PRIVILEGES,
};

/// Per-client session state tracking authentication and privileges.
///
/// Models the essential state of a connected client session without transport
/// details. A session is authenticated when `user_id` is `Some`.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ModelSession {
    /// The authenticated user ID, or `None` if not yet authenticated.
    pub user_id: Option<u32>,
    /// Privilege bitmap granted upon authentication.
    pub privileges: u64,
}

impl ModelSession {
    /// Returns `true` if this session has been authenticated.
    #[must_use]
    pub const fn is_authenticated(&self) -> bool { self.user_id.is_some() }

    /// Returns `true` if this session has the required privilege.
    ///
    /// An unauthenticated session never has any privileges.
    #[must_use]
    pub const fn has_privilege(&self, required: u64) -> bool {
        self.is_authenticated() && (self.privileges & required) == required
    }
}

/// Abstract request types representing classes of protocol transactions.
///
/// Each variant maps to a privilege requirement. The model uses these to
/// exercise privilege enforcement without modelling full protocol semantics.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequestType {
    /// Ping/keep-alive: no authentication required.
    Ping,
    /// Get user information: requires authentication but no special privilege.
    GetUserInfo,
    /// Get file listing: requires `DOWNLOAD_FILE` privilege.
    GetFileList,
    /// Get news categories: requires `NEWS_READ_ARTICLE` privilege.
    GetNewsCategories,
    /// Post a news article: requires `NEWS_POST_ARTICLE` privilege.
    PostNewsArticle,
    /// Get client info: requires `GET_CLIENT_INFO` privilege.
    GetClientInfo,
}

impl RequestType {
    /// Returns the privilege required to execute this request type.
    ///
    /// Returns `NO_PRIVILEGES` for requests that require only authentication
    /// (no specific privilege bit) or no authentication at all.
    #[must_use]
    pub const fn required_privilege(self) -> u64 {
        match self {
            Self::Ping | Self::GetUserInfo => NO_PRIVILEGES,
            Self::GetFileList => DOWNLOAD_FILE,
            Self::GetNewsCategories => NEWS_READ_ARTICLE,
            Self::PostNewsArticle => NEWS_POST_ARTICLE,
            Self::GetClientInfo => GET_CLIENT_INFO,
        }
    }

    /// Returns `true` if this request requires authentication.
    ///
    /// Ping does not require authentication; all other requests do.
    #[must_use]
    pub const fn requires_authentication(self) -> bool { !matches!(self, Self::Ping) }

    /// Returns `true` if this is a privileged operation (requires a specific
    /// privilege bit beyond just authentication).
    #[must_use]
    pub const fn is_privileged(self) -> bool { self.required_privilege() != NO_PRIVILEGES }

    /// Returns all request type variants for enumeration.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Ping,
            Self::GetUserInfo,
            Self::GetFileList,
            Self::GetNewsCategories,
            Self::PostNewsArticle,
            Self::GetClientInfo,
        ]
    }
}

/// A message queued for delivery to a client.
///
/// Wraps a request type to model out-of-order message delivery in the network.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ModelMessage {
    /// The request being delivered.
    pub request: RequestType,
}

impl ModelMessage {
    /// Creates a new message wrapping the given request.
    #[must_use]
    pub const fn new(request: RequestType) -> Self { Self { request } }
}

/// Observable effects recorded for invariant checking.
///
/// The model records effects as they occur, enabling temporal properties like
/// "authentication must precede any privileged effect".
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Effect {
    /// A client successfully authenticated.
    Authenticated {
        /// The client index.
        client: usize,
        /// The user ID assigned.
        user_id: u32,
    },
    /// A client logged out.
    LoggedOut {
        /// The client index.
        client: usize,
    },
    /// A request was rejected because the client is not authenticated.
    RejectedUnauthenticated {
        /// The client index.
        client: usize,
        /// The request that was rejected.
        request: RequestType,
    },
    /// A request was rejected due to insufficient privileges.
    RejectedInsufficientPrivilege {
        /// The client index.
        client: usize,
        /// The request that was rejected.
        request: RequestType,
        /// The privilege that was required.
        required: u64,
    },
    /// A privileged operation completed successfully.
    PrivilegedEffectCompleted {
        /// The client index.
        client: usize,
        /// The request that completed.
        request: RequestType,
        /// The privilege that was exercised.
        privilege: u64,
        /// The session's privilege set at delivery time.
        session_privileges: u64,
    },
    /// An unprivileged operation completed successfully.
    UnprivilegedEffectCompleted {
        /// The client index.
        client: usize,
        /// The request that completed.
        request: RequestType,
    },
}

impl Effect {
    /// Returns the client index associated with this effect.
    #[must_use]
    pub const fn client(&self) -> usize {
        match *self {
            Self::Authenticated { client, .. }
            | Self::LoggedOut { client }
            | Self::RejectedUnauthenticated { client, .. }
            | Self::RejectedInsufficientPrivilege { client, .. }
            | Self::PrivilegedEffectCompleted { client, .. }
            | Self::UnprivilegedEffectCompleted { client, .. } => client,
        }
    }

    /// Returns `true` if this is a privileged effect completion.
    #[must_use]
    pub const fn is_privileged_effect(&self) -> bool {
        matches!(self, Self::PrivilegedEffectCompleted { .. })
    }

    /// Returns `true` if this is an authentication event.
    #[must_use]
    pub const fn is_authentication(&self) -> bool { matches!(self, Self::Authenticated { .. }) }
}

/// Global system state containing all client sessions and queued messages.
///
/// This is the state type explored by Stateright. Each unique `SystemState`
/// represents a distinct point in the system's state space.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct SystemState {
    /// Per-client session state, indexed by client ID.
    pub sessions: Vec<ModelSession>,
    /// Per-client message queues for out-of-order delivery modelling.
    pub queues: Vec<Vec<ModelMessage>>,
    /// History of observable effects for temporal invariant checking.
    pub effects: Vec<Effect>,
}

impl SystemState {
    /// Creates a new system state with the specified number of clients.
    ///
    /// All clients start unauthenticated with empty queues.
    #[must_use]
    pub fn new(num_clients: usize) -> Self {
        Self {
            sessions: vec![ModelSession::default(); num_clients],
            queues: vec![Vec::new(); num_clients],
            effects: Vec::new(),
        }
    }

    /// Returns the number of clients in this system state.
    #[must_use]
    pub const fn num_clients(&self) -> usize { self.sessions.len() }

    /// Returns the session for the given client, if it exists.
    #[must_use]
    pub fn session(&self, client: usize) -> Option<&ModelSession> { self.sessions.get(client) }

    /// Returns the message queue for the given client, if it exists.
    #[must_use]
    pub fn queue(&self, client: usize) -> Option<&Vec<ModelMessage>> { self.queues.get(client) }

    /// Returns `true` if the given client has been authenticated at some point.
    ///
    /// Checks the effect history, not just current session state.
    #[must_use]
    pub fn client_has_authenticated(&self, client: usize) -> bool {
        self.effects
            .iter()
            .any(|e| matches!(e, Effect::Authenticated { client: c, .. } if *c == client))
    }

    /// Returns the index of the first authentication event for a client, if any.
    #[must_use]
    pub fn first_auth_index(&self, client: usize) -> Option<usize> {
        self.effects
            .iter()
            .position(|e| matches!(e, Effect::Authenticated { client: c, .. } if *c == client))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn model_session_defaults_unauthenticated() {
        let session = ModelSession::default();
        assert!(!session.is_authenticated());
        assert!(!session.has_privilege(DOWNLOAD_FILE));
    }

    #[test]
    fn authenticated_session_is_authenticated() {
        let session = ModelSession {
            user_id: Some(1),
            privileges: DOWNLOAD_FILE | NEWS_READ_ARTICLE,
        };
        assert!(session.is_authenticated());
    }

    #[test]
    fn authenticated_session_has_granted_privileges() {
        let session = ModelSession {
            user_id: Some(1),
            privileges: DOWNLOAD_FILE | NEWS_READ_ARTICLE,
        };
        assert!(session.has_privilege(DOWNLOAD_FILE));
        assert!(session.has_privilege(NEWS_READ_ARTICLE));
    }

    #[test]
    fn authenticated_session_lacks_ungranted_privileges() {
        let session = ModelSession {
            user_id: Some(1),
            privileges: DOWNLOAD_FILE | NEWS_READ_ARTICLE,
        };
        assert!(!session.has_privilege(NEWS_POST_ARTICLE));
    }

    #[test]
    fn unauthenticated_session_has_no_privileges() {
        let session = ModelSession {
            user_id: None,
            privileges: DOWNLOAD_FILE, // Even with bits set, no auth means no privs
        };
        assert!(!session.has_privilege(DOWNLOAD_FILE));
    }

    #[rstest]
    #[case(RequestType::Ping, NO_PRIVILEGES)]
    #[case(RequestType::GetUserInfo, NO_PRIVILEGES)]
    #[case(RequestType::GetFileList, DOWNLOAD_FILE)]
    #[case(RequestType::GetNewsCategories, NEWS_READ_ARTICLE)]
    #[case(RequestType::PostNewsArticle, NEWS_POST_ARTICLE)]
    fn request_type_privilege_requirements(#[case] request: RequestType, #[case] expected: u64) {
        assert_eq!(request.required_privilege(), expected);
    }

    #[rstest]
    #[case(RequestType::Ping, false)]
    #[case(RequestType::GetUserInfo, true)]
    #[case(RequestType::GetFileList, true)]
    #[case(RequestType::GetNewsCategories, true)]
    #[case(RequestType::PostNewsArticle, true)]
    fn request_type_auth_requirements(#[case] request: RequestType, #[case] expected: bool) {
        assert_eq!(request.requires_authentication(), expected);
    }

    #[rstest]
    #[case(RequestType::Ping, false)]
    #[case(RequestType::GetUserInfo, false)]
    #[case(RequestType::GetFileList, true)]
    #[case(RequestType::GetNewsCategories, true)]
    #[case(RequestType::PostNewsArticle, true)]
    fn request_type_privilege_classification(#[case] request: RequestType, #[case] expected: bool) {
        assert_eq!(request.is_privileged(), expected);
    }

    #[test]
    fn system_state_creates_correct_number_of_clients() {
        let state = SystemState::new(3);
        assert_eq!(state.num_clients(), 3);
    }

    #[test]
    fn system_state_initializes_collections() {
        let state = SystemState::new(3);
        assert_eq!(state.sessions.len(), 3);
        assert_eq!(state.queues.len(), 3);
        assert!(state.effects.is_empty());
    }

    #[test]
    fn system_state_clients_start_unauthenticated() {
        let state = SystemState::new(3);
        for session in &state.sessions {
            assert!(!session.is_authenticated());
        }
    }

    #[test]
    fn effect_client_extraction() {
        let effect = Effect::Authenticated {
            client: 2,
            user_id: 42,
        };
        assert_eq!(effect.client(), 2);

        let rejection = Effect::RejectedUnauthenticated {
            client: 1,
            request: RequestType::GetFileList,
        };
        assert_eq!(rejection.client(), 1);
    }
}
