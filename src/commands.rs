//! Parse and execute protocol transactions.
//!
//! This module converts incoming [`Transaction`] values into high level
//! [`Command`] variants and runs the appropriate handlers. Commands are used by
//! the connection handler to drive database operations and build reply
//! transactions.
use std::net::SocketAddr;

use futures_util::future::BoxFuture;
use tracing::error;

use crate::{
    db::{DbConnection, DbPool, PathLookupError, list_article_titles, list_names_at_path},
    field_id::FieldId,
    header_util::reply_header,
    login::handle_login,
    transaction::{
        FrameHeader,
        Transaction,
        decode_params,
        decode_params_map,
        encode_params,
        encode_vec_params,
        first_param_i32,
        first_param_string,
        required_param_i32,
        required_param_string,
    },
    transaction_type::TransactionType,
};

/// Error code used when the requested news path is unsupported.
pub const NEWS_ERR_PATH_UNSUPPORTED: u32 = 1;
/// Error code used when a request includes an unexpected payload.
pub const ERR_INVALID_PAYLOAD: u32 = 2;
/// Error code used for unexpected server-side failures.
pub const ERR_INTERNAL_SERVER: u32 = 3;

pub enum Command {
    Login {
        username: String,
        password: String,
        header: FrameHeader,
    },
    GetFileNameList {
        header: FrameHeader,
        payload: Vec<u8>,
    },
    GetNewsCategoryNameList {
        path: Option<String>,
        header: FrameHeader,
    },
    GetNewsArticleNameList {
        path: String,
        header: FrameHeader,
    },
    GetNewsArticleData {
        path: String,
        article_id: i32,
        header: FrameHeader,
    },
    PostNewsArticle {
        path: String,
        title: String,
        flags: i32,
        data_flavor: String,
        data: String,
        header: FrameHeader,
    },
    /// Request contained a payload when none was expected. The server
    /// responds with [`crate::commands::ERR_INVALID_PAYLOAD`].
    InvalidPayload {
        header: FrameHeader,
    },
    Unknown {
        header: FrameHeader,
    },
}

impl Command {
    /// Convert a [`Transaction`] into a [`Command`].
    ///
    /// # Errors
    /// Returns an error if required parameters are missing or cannot be parsed.
    #[must_use = "handle the result"]
    pub fn from_transaction(tx: Transaction) -> Result<Self, &'static str> {
        let ty = TransactionType::from(tx.header.ty);
        if !ty.allows_payload() && !tx.payload.is_empty() {
            return Ok(Command::InvalidPayload { header: tx.header });
        }
        match ty {
            TransactionType::Login => {
                let params = decode_params(&tx.payload).map_err(|_| "invalid params")?;
                let mut username = None;
                let mut password = None;
                for (id, data) in params {
                    match id {
                        FieldId::Login => {
                            username = Some(String::from_utf8(data).map_err(|_| "utf8")?);
                        }
                        FieldId::Password => {
                            password = Some(String::from_utf8(data).map_err(|_| "utf8")?);
                        }
                        _ => {}
                    }
                }
                Ok(Command::Login {
                    username: username.ok_or("missing username")?,
                    password: password.ok_or("missing password")?,
                    header: tx.header,
                })
            }
            TransactionType::GetFileNameList => Ok(Command::GetFileNameList {
                header: tx.header,
                payload: tx.payload,
            }),
            TransactionType::NewsCategoryNameList => {
                let params = decode_params_map(&tx.payload).map_err(|_| "invalid params")?;
                let path = first_param_string(&params, FieldId::NewsPath)?;
                Ok(Command::GetNewsCategoryNameList {
                    path,
                    header: tx.header,
                })
            }
            TransactionType::NewsArticleNameList => {
                let params = decode_params_map(&tx.payload).map_err(|_| "invalid params")?;
                let path = required_param_string(&params, FieldId::NewsPath, "missing path")?;
                Ok(Command::GetNewsArticleNameList {
                    path,
                    header: tx.header,
                })
            }
            TransactionType::NewsArticleData => {
                let params = decode_params_map(&tx.payload).map_err(|_| "invalid params")?;
                let path = required_param_string(&params, FieldId::NewsPath, "missing path")?;
                let id = required_param_i32(&params, FieldId::NewsArticleId, "missing id", "id")?;
                Ok(Command::GetNewsArticleData {
                    path,
                    article_id: id,
                    header: tx.header,
                })
            }
            TransactionType::PostNewsArticle => {
                let params = decode_params_map(&tx.payload).map_err(|_| "invalid params")?;
                let path = required_param_string(&params, FieldId::NewsPath, "missing path")?;
                let title = required_param_string(&params, FieldId::NewsTitle, "missing title")?;
                let flags =
                    first_param_i32(&params, FieldId::NewsArticleFlags, "flags")?.unwrap_or(0);
                let data_flavor =
                    required_param_string(&params, FieldId::NewsDataFlavor, "missing flavor")?;
                let data =
                    required_param_string(&params, FieldId::NewsArticleData, "missing data")?;
                Ok(Command::PostNewsArticle {
                    path,
                    title,
                    flags,
                    data_flavor,
                    data,
                    header: tx.header,
                })
            }
            _ => Ok(Command::Unknown { header: tx.header }),
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
            Command::Login {
                username,
                password,
                header,
            } => handle_login(peer, session, pool, username, password, header).await,
            Command::GetFileNameList { header, .. } => {
                let Some(user_id) = session.user_id else {
                    return Ok(Transaction {
                        header: reply_header(&header, 1, 0),
                        payload: Vec::new(),
                    });
                };
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
            Command::GetNewsCategoryNameList { header, path } => {
                handle_category_list(pool, header, path).await
            }
            Command::GetNewsArticleNameList { header, path } => {
                handle_article_titles(pool, header, path).await
            }
            Command::GetNewsArticleData {
                header,
                path,
                article_id,
            } => handle_article_data(pool, header, path, article_id).await,
            Command::PostNewsArticle {
                header,
                path,
                title,
                flags,
                data_flavor,
                data,
            } => handle_post_article(pool, header, path, title, flags, data_flavor, data).await,
            Command::InvalidPayload { header } => Ok(Transaction {
                header: reply_header(&header, ERR_INVALID_PAYLOAD, 0),
                payload: Vec::new(),
            }),
            Command::Unknown { header } => Ok(handle_unknown(peer, &header)),
        }
    }
}

/// Helper to execute a news database operation and build a reply transaction.
///
/// # Errors
/// Returns an error if database access fails or the operation itself errors.
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
                let payload = encode_vec_params(&params)?;
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

/// Create a new root article under the provided path.
///
/// # Errors
/// Returns an error if the path lookup fails or the database cannot be queried.
async fn handle_post_article(
    pool: DbPool,
    header: FrameHeader,
    path: String,
    title: String,
    flags: i32,
    data_flavor: String,
    data: String,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
    use crate::db::create_root_article;
    run_news_tx(pool, header, move |conn| {
        Box::pin(async move {
            let id = create_root_article(conn, &path, &title, flags, &data_flavor, &data).await?;
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
            error: 1,
            total_size: 0,
            data_size: 0,
        },
        payload: Vec::new(),
    };
    println!("{} sent unknown transaction: {}", peer, header.ty);
    reply
}
