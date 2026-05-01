//! Parse and execute protocol transactions.
//!
//! This module converts incoming [`Transaction`] values into high level
//! [`Command`] variants and runs the appropriate handlers. Commands are used by
//! the connection handler to drive database operations and build reply
//! transactions.

use std::net::SocketAddr;

mod handlers;
mod parsing;
mod support;

use diesel_async::pooled_connection::bb8::RunError;
use parsing::parse_command;
pub use support::ProcessContext;
pub(crate) use support::{
    CommandContext,
    UserInfoUpdate,
    check_privilege_and_run,
    privilege_error_reply,
};
use thiserror::Error;

use crate::{
    db::DbPool,
    handler::PrivilegeError,
    login::LoginRequest,
    news_handlers::{self, ArticleDataRequest, PostArticleRequest},
    server::outbound::OutboundError,
    transaction::{FrameHeader, Transaction, TransactionError},
};

/// Error code used when authentication is required but not present.
pub const ERR_NOT_AUTHENTICATED: u32 = 1;
/// Error code used when a request includes an unexpected payload.
pub const ERR_INVALID_PAYLOAD: u32 = 2;
/// Error code used for unexpected server-side failures.
pub const ERR_INTERNAL_SERVER: u32 = 3;
/// Error code used when the user lacks the required privilege.
pub const ERR_INSUFFICIENT_PRIVILEGES: u32 = 4;
/// Error code used when the requested news path is unsupported.
pub const NEWS_ERR_PATH_UNSUPPORTED: u32 = 5;
/// Error code used when a news article cannot be found.
pub const NEWS_ERR_ARTICLE_NOT_FOUND: u32 = 6;

/// Errors that can occur while processing commands.
#[derive(Debug, Error)]
pub enum CommandError {
    /// A database query failed.
    #[error("database error: {0}")]
    Database(#[from] diesel::result::Error),
    /// Connection pool access failed.
    #[error("pool error: {0}")]
    Pool(#[from] RunError),
    /// Transaction parsing or encoding failed.
    #[error("transaction error: {0}")]
    Transaction(#[from] TransactionError),
    /// Privilege checks failed unexpectedly.
    #[error("privilege error: {0}")]
    Privilege(#[from] PrivilegeError),
    /// Command processing invariants were violated.
    #[error("invariant violation: {0}")]
    Invariant(&'static str),
    /// Outbound transport failed to deliver a reply.
    #[error("outbound transport error: {0}")]
    Outbound(#[from] OutboundError),
}

/// High-level command representation parsed from incoming transactions.
///
/// Commands encapsulate the parameters and type information needed to
/// process client requests.
#[derive(Debug)]
pub enum Command {
    /// User login request with credentials.
    Login {
        /// Login request containing credentials and header.
        req: LoginRequest,
    },
    /// Request for the list of online users.
    GetUserNameList {
        /// Transaction frame header.
        header: FrameHeader,
    },
    /// Request for a user's info text by user id.
    GetClientInfoText {
        /// Transaction frame header.
        header: FrameHeader,
        /// Target user id.
        target_user_id: i32,
    },
    /// Update the current session's visible user metadata.
    SetClientUserInfo {
        /// Transaction frame header.
        header: FrameHeader,
        /// Requested metadata changes.
        update: UserInfoUpdate,
    },
    /// Request for the list of available files.
    GetFileNameList {
        /// Transaction frame header.
        header: FrameHeader,
    },
    /// Request for news category names at a given path.
    GetNewsCategoryNameList {
        /// News hierarchy path (optional for root).
        path: Option<String>,
        /// Transaction frame header.
        header: FrameHeader,
    },
    /// Request for article titles within a news category.
    GetNewsArticleNameList {
        /// News category path.
        path: String,
        /// Transaction frame header.
        header: FrameHeader,
    },
    /// Request for a specific news article's content.
    GetNewsArticleData {
        /// News category path.
        path: String,
        /// Article identifier.
        article_id: i32,
        /// Transaction frame header.
        header: FrameHeader,
    },
    /// Request to create a new news article.
    PostNewsArticle {
        /// Article posting request containing path, title, and content.
        req: PostArticleRequest,
        /// Transaction frame header.
        header: FrameHeader,
    },
    /// Request contained a payload when none was expected. The server
    /// responds with [`crate::commands::ERR_INVALID_PAYLOAD`].
    InvalidPayload {
        /// Transaction frame header.
        header: FrameHeader,
    },
    /// Unrecognised transaction type.
    Unknown {
        /// Transaction frame header.
        header: FrameHeader,
    },
}

impl Command {
    /// Convert a [`Transaction`] into a [`Command`].
    ///
    /// # Errors
    /// Returns an error if required parameters are missing or cannot be parsed.
    pub fn from_transaction(tx: Transaction) -> Result<Self, TransactionError> { parse_command(tx) }

    /// Execute the command using the provided context.
    ///
    /// # Errors
    /// Returns an error if database access fails or the command cannot be
    /// handled.
    pub async fn process(self, context: ProcessContext<'_>) -> Result<Transaction, CommandError> {
        let ProcessContext {
            peer,
            pool,
            session,
            presence,
            presence_connection_id,
        } = context;
        let mut transport = crate::server::outbound::ReplyBuffer::new();
        let messaging = crate::server::outbound::NoopOutboundMessaging;
        self.process_with_outbound(CommandContext {
            peer,
            pool,
            session,
            transport: &mut transport,
            messaging: &messaging,
            presence,
            presence_connection_id,
        })
        .await?;
        transport
            .take_reply()
            .ok_or(CommandError::Outbound(OutboundError::ReplyMissing))
    }

    /// Execute the command using outbound transport and messaging adapters.
    ///
    /// # Errors
    /// Returns an error if database access fails or the command cannot be
    /// handled.
    pub(crate) async fn process_with_outbound(
        self,
        context: CommandContext<'_>,
    ) -> Result<(), CommandError> {
        match self {
            Self::Login { .. }
            | Self::GetUserNameList { .. }
            | Self::GetClientInfoText { .. }
            | Self::SetClientUserInfo { .. } => self.process_presence_command(context).await,
            command => {
                let CommandContext {
                    peer,
                    pool,
                    session,
                    transport,
                    ..
                } = context;
                let reply = command.execute(peer, pool, session).await?;
                transport.send_reply(reply)?;
                Ok(())
            }
        }
    }

    async fn process_presence_command(
        self,
        context: CommandContext<'_>,
    ) -> Result<(), CommandError> {
        match self {
            Self::Login { req } => Self::process_login_with_presence(context, req).await,
            Self::GetUserNameList { header } => Self::process_get_user_name_list(context, &header),
            Self::GetClientInfoText {
                header,
                target_user_id,
            } => Self::process_get_client_info_text(context, header, target_user_id).await,
            Self::SetClientUserInfo { header, update } => {
                Self::process_set_client_user_info(context, header, update).await
            }
            _ => Err(CommandError::Invariant(
                "non-presence command passed to presence dispatcher",
            )),
        }
    }

    async fn execute(
        self,
        peer: SocketAddr,
        pool: DbPool,
        session: &mut crate::handler::Session,
    ) -> Result<Transaction, CommandError> {
        match self {
            Self::Login { req } => Self::process_login(peer, pool, session, req).await,
            Self::GetFileNameList { header } => {
                Self::process_get_file_name_list(pool, session, header).await
            }
            Self::GetNewsCategoryNameList { header, path } => {
                news_handlers::process_category_name_list(pool, session, header, path).await
            }
            Self::GetNewsArticleNameList { header, path } => {
                news_handlers::process_article_name_list(pool, session, header, path).await
            }
            Self::GetNewsArticleData {
                header,
                path,
                article_id,
            } => {
                let req = ArticleDataRequest { path, article_id };
                news_handlers::process_article_data(pool, session, header, req).await
            }
            Self::PostNewsArticle { header, req } => {
                news_handlers::process_post_article(pool, session, header, req).await
            }
            Self::GetUserNameList { .. }
            | Self::GetClientInfoText { .. }
            | Self::SetClientUserInfo { .. } => Err(CommandError::Invariant(
                "presence command should be handled before execute",
            )),
            Self::InvalidPayload { header } => Ok(Self::process_invalid_payload(header)),
            Self::Unknown { header } => Ok(Self::process_unknown(peer, header)),
        }
    }
}

#[cfg(test)]
mod tests;
