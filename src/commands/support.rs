//! Shared command support types and privilege helpers.

use std::future::Future;

use super::{CommandError, ERR_INSUFFICIENT_PRIVILEGES, ERR_NOT_AUTHENTICATED};
use crate::{
    connection_flags::ConnectionFlags,
    db::DbPool,
    handler::PrivilegeError,
    header_util::reply_header,
    presence::PresenceRegistry,
    privileges::Privileges,
    server::outbound::{OutboundMessaging, OutboundTransport},
    transaction::{FrameHeader, Transaction},
};

/// Execution context for command processing with outbound adapters.
pub(crate) struct CommandContext<'a> {
    /// Remote peer address.
    pub peer: std::net::SocketAddr,
    /// Database connection pool.
    pub pool: DbPool,
    /// Mutable session state for the connection.
    pub session: &'a mut crate::handler::Session,
    /// Outbound transport for replies.
    pub transport: &'a mut dyn OutboundTransport,
    /// Outbound messaging adapter for pushes.
    pub messaging: &'a dyn OutboundMessaging,
    /// Shared presence registry for online-user state.
    pub presence: &'a PresenceRegistry,
}

/// Execution context for command processing without external outbound adapters.
pub struct ProcessContext<'a> {
    /// Remote peer address.
    pub peer: std::net::SocketAddr,
    /// Database connection pool.
    pub pool: DbPool,
    /// Mutable session state for the connection.
    pub session: &'a mut crate::handler::Session,
    /// Shared presence registry for online-user state.
    pub presence: &'a PresenceRegistry,
}

/// User-visible metadata updates accepted by `121` and `304`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UserInfoUpdate {
    /// Replacement nickname, if any.
    pub display_name: Option<String>,
    /// Replacement icon identifier, if any.
    pub icon_id: Option<u16>,
    /// Replacement connection flags, if any.
    pub options: Option<ConnectionFlags>,
    /// Replacement automatic-response text, if any.
    pub auto_response: Option<String>,
}

/// Build an error reply for a privilege check failure.
pub(crate) fn privilege_error_reply(header: &FrameHeader, err: PrivilegeError) -> Transaction {
    let error_code = match err {
        PrivilegeError::NotAuthenticated => ERR_NOT_AUTHENTICATED,
        PrivilegeError::InsufficientPrivileges(_) => ERR_INSUFFICIENT_PRIVILEGES,
    };
    Transaction {
        header: reply_header(header, error_code, 0),
        payload: Vec::new(),
    }
}

/// Check privileges and run a handler, mapping failures to error replies.
pub(crate) async fn check_privilege_and_run<F, Fut>(
    session: &crate::handler::Session,
    header: &FrameHeader,
    privilege: Privileges,
    handler: F,
) -> Result<Transaction, CommandError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Transaction, CommandError>>,
{
    if let Err(error) = session.require_privilege(privilege) {
        return Ok(privilege_error_reply(header, error));
    }
    handler().await
}
