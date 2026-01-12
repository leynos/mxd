//! Parse and execute protocol transactions.
//!
//! This module converts incoming [`Transaction`] values into high level
//! [`Command`] variants and runs the appropriate handlers. Commands are used by
//! the connection handler to drive database operations and build reply
//! transactions.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use std::net::SocketAddr;

use futures_util::future::BoxFuture;
use tracing::error;

use crate::{
    db::{
        CreateRootArticleParams,
        DbConnection,
        DbPool,
        PathLookupError,
        create_root_article,
        list_article_titles,
        list_names_at_path,
    },
    field_id::FieldId,
    handler::PrivilegeError,
    header_util::reply_header,
    login::{LoginRequest, handle_login},
    privileges::Privileges,
    transaction::{
        FrameHeader,
        Transaction,
        TransactionError,
        decode_params,
        decode_params_map,
        encode_params,
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
fn privilege_error_reply(header: &FrameHeader, err: PrivilegeError) -> Transaction {
    let error_code = match err {
        PrivilegeError::NotAuthenticated => ERR_NOT_AUTHENTICATED,
        PrivilegeError::InsufficientPrivileges(_) => ERR_INSUFFICIENT_PRIVILEGES,
    };
    Transaction {
        header: reply_header(header, error_code, 0),
        payload: Vec::new(),
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
                let req = LoginRequest {
                    username,
                    password,
                    header,
                };
                handle_login(peer, session, pool, req).await
            }
            Self::GetFileNameList { header, .. } => {
                if let Err(e) = session.require_privilege(Privileges::DOWNLOAD_FILE) {
                    return Ok(privilege_error_reply(&header, e));
                }
                let user_id = session
                    .user_id
                    .expect("require_privilege guarantees authentication");
                let mut conn = pool.get().await?;
                let files = crate::db::list_files_for_user(&mut conn, user_id).await?;
                let params: Vec<(FieldId, &[u8])> = files
                    .iter()
                    .map(|f| (FieldId::FileName, f.name.as_bytes()))
                    .collect();
                let payload = encode_params(&params)?;
                Ok(Transaction {
                    header: reply_header(&header, 0, payload.len()),
                    payload,
                })
            }
            Self::GetNewsCategoryNameList { header, path } => {
                if let Err(e) = session.require_privilege(Privileges::NEWS_READ_ARTICLE) {
                    return Ok(privilege_error_reply(&header, e));
                }
                handle_category_list(pool, header, path).await
            }
            Self::GetNewsArticleNameList { header, path } => {
                if let Err(e) = session.require_privilege(Privileges::NEWS_READ_ARTICLE) {
                    return Ok(privilege_error_reply(&header, e));
                }
                handle_article_titles(pool, header, path).await
            }
            Self::GetNewsArticleData {
                header,
                path,
                article_id,
            } => {
                if let Err(e) = session.require_privilege(Privileges::NEWS_READ_ARTICLE) {
                    return Ok(privilege_error_reply(&header, e));
                }
                handle_article_data(pool, header, path, article_id).await
            }
            Self::PostNewsArticle {
                header,
                path,
                title,
                flags,
                data_flavor,
                data,
            } => {
                if let Err(e) = session.require_privilege(Privileges::NEWS_POST_ARTICLE) {
                    return Ok(privilege_error_reply(&header, e));
                }
                let req = PostArticleRequest {
                    path,
                    title,
                    flags,
                    data_flavor,
                    data,
                };
                handle_post_article(pool, header, req).await
            }
            Self::InvalidPayload { header } => Ok(Transaction {
                header: reply_header(&header, ERR_INVALID_PAYLOAD, 0),
                payload: Vec::new(),
            }),
            Self::Unknown { header } => Ok(handle_unknown(peer, &header)),
        }
    }
}

/// Helper to execute a news database operation and build a reply transaction.
///
/// # Errors
/// Returns an error if database access fails or the operation itself errors.
#[expect(
    clippy::cognitive_complexity,
    reason = "refactoring would reduce readability"
)]
async fn run_news_tx<F>(
    pool: DbPool,
    header: FrameHeader,
    op: F,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>>
where
    for<'c> F: FnOnce(
            &'c mut DbConnection,
        ) -> BoxFuture<'c, Result<Vec<(FieldId, Vec<u8>)>, PathLookupError>>
        + Send
        + 'static,
{
    match pool.get().await {
        Ok(mut conn) => match op(&mut conn).await {
            Ok(params) => {
                let payload = encode_params(&params)?;
                Ok(Transaction {
                    header: reply_header(&header, 0, payload.len()),
                    payload,
                })
            }
            Err(e) => Ok(news_error_reply(&header, e)),
        },
        Err(e) => {
            error!(%e, "failed to get database connection");
            Ok(Transaction {
                header: reply_header(&header, ERR_INTERNAL_SERVER, 0),
                payload: Vec::new(),
            })
        }
    }
}

#[expect(
    clippy::cognitive_complexity,
    reason = "error handling requires matching multiple error types"
)]
fn news_error_reply(header: &FrameHeader, err: PathLookupError) -> Transaction {
    match err {
        PathLookupError::InvalidPath => Transaction {
            header: reply_header(header, NEWS_ERR_PATH_UNSUPPORTED, 0),
            payload: Vec::new(),
        },
        PathLookupError::Diesel(e) => {
            error!("database error: {e}");
            Transaction {
                header: reply_header(header, ERR_INTERNAL_SERVER, 0),
                payload: Vec::new(),
            }
        }
        PathLookupError::Serde(e) => {
            error!("serialization error: {e}");
            Transaction {
                header: reply_header(header, ERR_INTERNAL_SERVER, 0),
                payload: Vec::new(),
            }
        }
    }
}

/// Retrieve the list of category names for a given news path.
///
/// # Errors
/// Returns an error if the path lookup fails or the database cannot be queried.
async fn handle_category_list(
    pool: DbPool,
    header: FrameHeader,
    path: Option<String>,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
    run_news_tx(pool, header, move |conn| {
        Box::pin(async move {
            let names = list_names_at_path(conn, path.as_deref()).await?;
            let params = names
                .into_iter()
                .map(|c| (FieldId::NewsCategory, c.into_bytes()))
                .collect();
            Ok(params)
        })
    })
    .await
}

/// Retrieve the titles of articles in a news category.
///
/// # Errors
/// Returns an error if the path lookup fails or the database cannot be queried.
async fn handle_article_titles(
    pool: DbPool,
    header: FrameHeader,
    path: String,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
    run_news_tx(pool, header, move |conn| {
        Box::pin(async move {
            let names = list_article_titles(conn, &path).await?;
            let params = names
                .into_iter()
                .map(|t| (FieldId::NewsArticle, t.into_bytes()))
                .collect();
            Ok(params)
        })
    })
    .await
}

/// Retrieve a specific news article's data.
///
/// # Errors
/// Returns an error if the path lookup fails or the database cannot be queried.
async fn handle_article_data(
    pool: DbPool,
    header: FrameHeader,
    path: String,
    article_id: i32,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
    use crate::db::get_article;
    run_news_tx(pool, header, move |conn| {
        Box::pin(async move {
            let article = get_article(conn, &path, article_id).await?;
            #[expect(
                clippy::shadow_reuse,
                reason = "intentional pattern for option handling"
            )]
            let Some(article) = article else {
                return Err(PathLookupError::InvalidPath);
            };

            let mut params: Vec<(FieldId, Vec<u8>)> = Vec::new();
            params.push((FieldId::NewsTitle, article.title.into_bytes()));
            if let Some(p) = article.poster {
                params.push((FieldId::NewsPoster, p.into_bytes()));
            }
            params.push((
                FieldId::NewsDate,
                article
                    .posted_at
                    .and_utc()
                    .timestamp_millis()
                    .to_be_bytes()
                    .to_vec(),
            ));
            if let Some(prev) = article.prev_article_id {
                params.push((FieldId::NewsPrevId, prev.to_be_bytes().to_vec()));
            }
            if let Some(next) = article.next_article_id {
                params.push((FieldId::NewsNextId, next.to_be_bytes().to_vec()));
            }
            if let Some(parent) = article.parent_article_id {
                params.push((FieldId::NewsParentId, parent.to_be_bytes().to_vec()));
            }
            if let Some(child) = article.first_child_article_id {
                params.push((FieldId::NewsFirstChildId, child.to_be_bytes().to_vec()));
            }
            params.push((
                FieldId::NewsArticleFlags,
                article.flags.to_be_bytes().to_vec(),
            ));
            params.push((
                FieldId::NewsDataFlavor,
                article
                    .data_flavor
                    .as_deref()
                    .unwrap_or("text/plain")
                    .as_bytes()
                    .to_vec(),
            ));
            if let Some(data) = article.data {
                params.push((FieldId::NewsArticleData, data.into_bytes()));
            }
            Ok(params)
        })
    })
    .await
}

/// Parameters for posting a new news article.
#[derive(Debug, PartialEq, Eq)]
struct PostArticleRequest {
    path: String,
    title: String,
    flags: i32,
    data_flavor: String,
    data: String,
}

impl PostArticleRequest {
    /// Build database parameters from this request.
    ///
    /// The returned [`CreateRootArticleParams`] borrows from `self` and contains
    /// all fields except `path`, which is used separately for path lookup.
    fn to_db_params(&self) -> CreateRootArticleParams<'_> {
        CreateRootArticleParams {
            title: &self.title,
            flags: self.flags,
            data_flavor: &self.data_flavor,
            data: &self.data,
        }
    }
}

/// Create a new root article under the provided path.
///
/// # Errors
/// Returns an error if the path lookup fails or the database cannot be queried.
async fn handle_post_article(
    pool: DbPool,
    header: FrameHeader,
    req: PostArticleRequest,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
    run_news_tx(pool, header, move |conn| {
        Box::pin(async move {
            let id = create_root_article(conn, &req.path, req.to_db_params()).await?;
            let bytes = id.to_be_bytes();
            Ok(vec![(FieldId::NewsArticleId, bytes.to_vec())])
        })
    })
    .await
}

fn handle_unknown(peer: SocketAddr, header: &FrameHeader) -> Transaction {
    let reply = Transaction {
        header: FrameHeader {
            flags: 0,
            is_reply: 1,
            ty: header.ty,
            id: header.id,
            error: ERR_INTERNAL_SERVER,
            total_size: 0,
            data_size: 0,
        },
        payload: Vec::new(),
    };
    tracing::warn!(%peer, ty = %header.ty, "unknown transaction");
    reply
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// Returns valid login parameters for testing.
    fn valid_login_params() -> Vec<(FieldId, Vec<u8>)> {
        vec![
            (FieldId::Login, b"alice".to_vec()),
            (FieldId::Password, b"secret".to_vec()),
        ]
    }

    /// Asserts that credentials match expected valid values.
    fn assert_valid_credentials(creds: &LoginCredentials) {
        assert_eq!(creds.username, "alice");
        assert_eq!(creds.password, "secret");
    }

    #[test]
    fn parse_login_params_both_fields_valid() {
        let params = valid_login_params();
        let result = parse_login_params(params).expect("should parse");
        assert_valid_credentials(&result);
    }

    #[test]
    fn parse_login_params_missing_username() {
        let params = vec![(FieldId::Password, b"secret".to_vec())];
        let result = parse_login_params(params);
        assert!(matches!(
            result,
            Err(TransactionError::MissingField(FieldId::Login))
        ));
    }

    #[test]
    fn parse_login_params_missing_password() {
        let params = vec![(FieldId::Login, b"alice".to_vec())];
        let result = parse_login_params(params);
        assert!(matches!(
            result,
            Err(TransactionError::MissingField(FieldId::Password))
        ));
    }

    #[rstest]
    #[case(FieldId::Login, FieldId::Login)]
    #[case(FieldId::Password, FieldId::Password)]
    fn parse_login_params_invalid_utf8(
        #[case] invalid_field: FieldId,
        #[case] expected_error_field: FieldId,
    ) {
        let params = vec![
            (
                FieldId::Login,
                if invalid_field == FieldId::Login {
                    vec![0xff, 0xfe]
                } else {
                    b"alice".to_vec()
                },
            ),
            (
                FieldId::Password,
                if invalid_field == FieldId::Password {
                    vec![0xff, 0xfe]
                } else {
                    b"secret".to_vec()
                },
            ),
        ];
        let result = parse_login_params(params);
        assert!(matches!(
            result,
            Err(TransactionError::InvalidParamValue(field)) if field == expected_error_field
        ));
    }

    #[test]
    fn parse_login_params_ignores_extra_fields() {
        let mut params = valid_login_params();
        params.push((FieldId::NewsPath, b"/news".to_vec()));
        let result = parse_login_params(params).expect("should parse");
        assert_valid_credentials(&result);
    }

    /// Returns a `PostArticleRequest` with sensible default values for testing.
    fn default_post_article_request() -> PostArticleRequest {
        PostArticleRequest {
            path: "/news".to_string(),
            title: "Test Article".to_string(),
            flags: 0,
            data_flavor: "text/plain".to_string(),
            data: "Test content".to_string(),
        }
    }

    #[test]
    fn post_article_request_to_db_params_maps_title() {
        let req = PostArticleRequest {
            title: "Hello World".to_string(),
            ..default_post_article_request()
        };
        let params = req.to_db_params();
        assert_eq!(params.title, "Hello World");
    }

    #[test]
    fn post_article_request_to_db_params_maps_flags() {
        let req = PostArticleRequest {
            flags: 42,
            ..default_post_article_request()
        };
        let params = req.to_db_params();
        assert_eq!(params.flags, 42);
    }

    #[test]
    fn post_article_request_to_db_params_maps_data_flavor() {
        let req = PostArticleRequest {
            data_flavor: "text/html".to_string(),
            ..default_post_article_request()
        };
        let params = req.to_db_params();
        assert_eq!(params.data_flavor, "text/html");
    }

    #[test]
    fn post_article_request_to_db_params_maps_data() {
        let req = PostArticleRequest {
            data: "Article body content".to_string(),
            ..default_post_article_request()
        };
        let params = req.to_db_params();
        assert_eq!(params.data, "Article body content");
    }

    #[test]
    fn post_article_request_to_db_params_excludes_path() {
        let req = PostArticleRequest {
            path: "/news/category".to_string(),
            ..default_post_article_request()
        };
        let params = req.to_db_params();
        // Path is not part of CreateRootArticleParams; it's used separately
        // for the path lookup. Verify the other fields are correctly mapped.
        assert_eq!(params.title, "Test Article");
        assert_eq!(params.flags, 0);
        assert_eq!(params.data_flavor, "text/plain");
        assert_eq!(params.data, "Test content");
    }
}
