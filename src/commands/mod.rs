//! Parse and execute protocol transactions.
//!
//! This module converts incoming [`Transaction`] values into high level
//! [`Command`] variants and runs the appropriate handlers. Commands are used by
//! the connection handler to drive database operations and build reply
//! transactions.

use std::net::SocketAddr;

mod handlers;

use crate::{
    db::DbPool,
    field_id::FieldId,
    handler::PrivilegeError,
    header_util::reply_header,
    news_handlers,
    transaction::{
        FrameHeader,
        Transaction,
        TransactionError,
        decode_params,
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
/// Error code used when the requested news path is unsupported.
pub const NEWS_ERR_PATH_UNSUPPORTED: u32 = 1;
/// Error code used when a request includes an unexpected payload.
pub const ERR_INVALID_PAYLOAD: u32 = 2;
/// Error code used for unexpected server-side failures.
pub const ERR_INTERNAL_SERVER: u32 = 3;
/// Error code used when the user lacks the required privilege.
pub const ERR_INSUFFICIENT_PRIVILEGES: u32 = 4;

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

/// Context passed to all command handlers containing shared infrastructure.
struct HandlerContext<'a> {
    pool: DbPool,
    session: &'a mut crate::handler::Session,
    header: FrameHeader,
}

impl<'a> HandlerContext<'a> {
    const fn new(
        pool: DbPool,
        session: &'a mut crate::handler::Session,
        header: FrameHeader,
    ) -> Self {
        Self {
            pool,
            session,
            header,
        }
    }
}

/// High-level command representation parsed from incoming transactions.
///
/// Commands encapsulate the parameters and type information needed to
/// process client requests.
pub enum Command {
    /// User login request with credentials.
    Login {
        /// Username for authentication.
        username: String,
        /// Password for authentication.
        password: String,
        /// Transaction frame header.
        header: FrameHeader,
    },
    /// Request for the list of available files.
    GetFileNameList {
        /// Transaction frame header.
        header: FrameHeader,
        /// Raw payload bytes.
        payload: Vec<u8>,
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
        /// News category path.
        path: String,
        /// Article title.
        title: String,
        /// Article flags.
        flags: i32,
        /// Data content type.
        data_flavor: String,
        /// Article content.
        data: String,
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

/// Extract username and password from login parameters.
fn parse_login_params(
    params: Vec<(FieldId, Vec<u8>)>,
) -> Result<LoginCredentials, TransactionError> {
    let mut username = None;
    let mut password = None;

    for (id, data) in params {
        match id {
            FieldId::Login => {
                username = Some(
                    String::from_utf8(data)
                        .map_err(|_| TransactionError::InvalidParamValue(FieldId::Login))?,
                );
            }
            FieldId::Password => {
                password = Some(
                    String::from_utf8(data)
                        .map_err(|_| TransactionError::InvalidParamValue(FieldId::Password))?,
                );
            }
            _ => {}
        }
    }

    Ok(LoginCredentials {
        username: username.ok_or(TransactionError::MissingField(FieldId::Login))?,
        password: password.ok_or(TransactionError::MissingField(FieldId::Password))?,
    })
}

impl Command {
    /// Convert a [`Transaction`] into a [`Command`].
    ///
    /// # Errors
    /// Returns an error if required parameters are missing or cannot be parsed.
    #[must_use = "handle the result"]
    pub fn from_transaction(tx: Transaction) -> Result<Self, TransactionError> {
        let ty = TransactionType::from(tx.header.ty);
        if !ty.allows_payload() && !tx.payload.is_empty() {
            return Ok(Self::InvalidPayload { header: tx.header });
        }
        match ty {
            TransactionType::Login => {
                let params = decode_params(&tx.payload)?;
                let creds = parse_login_params(params)?;
                Ok(Self::Login {
                    username: creds.username,
                    password: creds.password,
                    header: tx.header,
                })
            }
            TransactionType::GetFileNameList => Ok(Self::GetFileNameList {
                header: tx.header,
                payload: tx.payload,
            }),
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
                let flags = first_param_i32(&params, FieldId::NewsArticleFlags)?.unwrap_or(0);
                let data_flavor = required_param_string(&params, FieldId::NewsDataFlavor)?;
                let data = required_param_string(&params, FieldId::NewsArticleData)?;
                Ok(Self::PostNewsArticle {
                    path,
                    title,
                    flags,
                    data_flavor,
                    data,
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
    #[must_use = "handle the result"]
    pub async fn process(
        self,
        peer: SocketAddr,
        pool: DbPool,
        session: &mut crate::handler::Session,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
        match self {
            Self::Login {
                username,
                password,
                header,
            } => {
                let ctx = HandlerContext::new(pool, session, header);
                Self::process_login(peer, ctx, username, password).await
            }
            Self::GetFileNameList { header, .. } => {
                let ctx = HandlerContext::new(pool, session, header);
                Self::process_get_file_name_list(ctx).await
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
            } => news_handlers::process_article_data(pool, session, header, path, article_id).await,
            Self::PostNewsArticle {
                header,
                path,
                title,
                flags,
                data_flavor,
                data,
            } => {
                news_handlers::process_post_article(
                    pool,
                    session,
                    header,
                    path,
                    title,
                    flags,
                    data_flavor,
                    data,
                )
                .await
            }
            Self::InvalidPayload { header } => Ok(Self::process_invalid_payload(header)),
            Self::Unknown { header } => Ok(Self::process_unknown(peer, header)),
        }
    }
}

#[cfg(test)]
mod tests;
