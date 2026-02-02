//! Parse and execute protocol transactions.
//!
//! This module converts incoming [`Transaction`] values into high level
//! [`Command`] variants and runs the appropriate handlers. Commands are used by
//! the connection handler to drive database operations and build reply
//! transactions.

use std::{future::Future, net::SocketAddr};

mod handlers;

use diesel_async::pooled_connection::bb8::RunError;
use thiserror::Error;

use crate::{
    db::DbPool,
    field_id::FieldId,
    handler::PrivilegeError,
    header_util::reply_header,
    login::LoginRequest,
    news_handlers::{self, ArticleDataRequest, PostArticleRequest},
    privileges::Privileges,
    server::outbound::{OutboundError, OutboundMessaging, OutboundTransport},
    transaction::{
        FrameHeader,
        Transaction,
        TransactionError,
        decode_params_map,
        first_param_i32,
        first_param_string,
        required_param_i32,
        required_param_string,
    },
    transaction_type::TransactionType,
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

/// Execution context for command processing with outbound adapters.
pub(crate) struct CommandContext<'a> {
    /// Remote peer address.
    pub peer: SocketAddr,
    /// Database connection pool.
    pub pool: DbPool,
    /// Mutable session state for the connection.
    pub session: &'a mut crate::handler::Session,
    /// Outbound transport for replies.
    pub transport: &'a mut dyn OutboundTransport,
    /// Outbound messaging adapter for pushes.
    pub messaging: &'a dyn OutboundMessaging,
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
    if let Err(e) = session.require_privilege(privilege) {
        return Ok(privilege_error_reply(header, e));
    }
    handler().await
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

/// Parsed login credentials extracted from transaction parameters.
#[derive(Debug, PartialEq, Eq)]
struct LoginCredentials {
    /// Username for authentication.
    username: String,
    /// Password for authentication.
    password: String,
}

/// Extract username and password from login payload parameters.
fn parse_login_params(payload: &[u8]) -> Result<LoginCredentials, TransactionError> {
    let params = decode_params_map(payload)?;
    Ok(LoginCredentials {
        username: required_param_string(&params, FieldId::Login)?,
        password: required_param_string(&params, FieldId::Password)?,
    })
}

impl Command {
    /// Convert a [`Transaction`] into a [`Command`].
    ///
    /// # Errors
    /// Returns an error if required parameters are missing or cannot be parsed.
    pub fn from_transaction(tx: Transaction) -> Result<Self, TransactionError> {
        let ty = TransactionType::from(tx.header.ty);
        if !ty.allows_payload() && !tx.payload.is_empty() {
            return Ok(Self::InvalidPayload { header: tx.header });
        }
        match ty {
            TransactionType::Login => {
                let creds = parse_login_params(&tx.payload)?;
                Ok(Self::Login {
                    req: LoginRequest {
                        username: creds.username,
                        password: creds.password,
                        header: tx.header,
                    },
                })
            }
            TransactionType::GetFileNameList => Ok(Self::GetFileNameList { header: tx.header }),
            TransactionType::NewsCategoryNameList => {
                let params = decode_params_map(&tx.payload)?;
                let path = first_param_string(&params, FieldId::NewsPath)?;
                Ok(Self::GetNewsCategoryNameList {
                    path,
                    header: tx.header,
                })
            }
            TransactionType::NewsArticleNameList => {
                let params = decode_params_map(&tx.payload)?;
                let path = required_param_string(&params, FieldId::NewsPath)?;
                Ok(Self::GetNewsArticleNameList {
                    path,
                    header: tx.header,
                })
            }
            TransactionType::NewsArticleData => {
                let params = decode_params_map(&tx.payload)?;
                let path = required_param_string(&params, FieldId::NewsPath)?;
                let id = required_param_i32(&params, FieldId::NewsArticleId)?;
                Ok(Self::GetNewsArticleData {
                    path,
                    article_id: id,
                    header: tx.header,
                })
            }
            TransactionType::PostNewsArticle => {
                let params = decode_params_map(&tx.payload)?;
                let path = required_param_string(&params, FieldId::NewsPath)?;
                let title = required_param_string(&params, FieldId::NewsTitle)?;
                // NewsArticleFlags (field 334): 0 = normal post; bit flags may indicate
                // locked/announcement status per Hotline protocol.
                let flags = first_param_i32(&params, FieldId::NewsArticleFlags)?.unwrap_or(0);
                let data_flavor = required_param_string(&params, FieldId::NewsDataFlavor)?;
                let data = required_param_string(&params, FieldId::NewsArticleData)?;
                Ok(Self::PostNewsArticle {
                    req: PostArticleRequest {
                        path,
                        title,
                        flags,
                        data_flavor,
                        data,
                    },
                    header: tx.header,
                })
            }
            _ => Ok(Self::Unknown { header: tx.header }),
        }
    }

    /// Execute the command using the provided context.
    ///
    /// # Errors
    /// Returns an error if database access fails or the command cannot be
    /// handled.
    pub async fn process(
        self,
        peer: SocketAddr,
        pool: DbPool,
        session: &mut crate::handler::Session,
    ) -> Result<Transaction, CommandError> {
        let mut transport = crate::server::outbound::ReplyBuffer::new();
        let messaging = crate::server::outbound::NoopOutboundMessaging;
        self.process_with_outbound(CommandContext {
            peer,
            pool,
            session,
            transport: &mut transport,
            messaging: &messaging,
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
        let CommandContext {
            peer,
            pool,
            session,
            transport,
            messaging,
        } = context;
        let reply = self.execute(peer, pool, session).await?;
        // TODO: use `messaging` for server-initiated notifications.
        let _ = messaging;
        transport.send_reply(reply)?;
        Ok(())
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
            Self::InvalidPayload { header } => Ok(Self::process_invalid_payload(header)),
            Self::Unknown { header } => Ok(Self::process_unknown(peer, header)),
        }
    }
}

#[cfg(test)]
mod tests;
