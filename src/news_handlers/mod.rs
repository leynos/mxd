//! News command helpers and database operations.
//!
//! These helpers keep news-related transactions and database access logic
//! grouped together for reuse by command processing.
#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use futures_util::future::BoxFuture;
use tracing::error;

use crate::{
    commands::{
        CommandError,
        ERR_INTERNAL_SERVER,
        NEWS_ERR_ARTICLE_NOT_FOUND,
        NEWS_ERR_PATH_UNSUPPORTED,
        check_privilege_and_run,
    },
    db::{
        CreateRootArticleParams,
        DbConnection,
        DbPool,
        PathLookupError,
        create_root_article,
        get_article,
        list_article_titles,
        list_names_at_path,
    },
    field_id::FieldId,
    handler::Session,
    header_util::reply_header,
    models::Article,
    privileges::Privileges,
    transaction::{FrameHeader, Transaction, encode_params},
};

/// Macro to reduce boilerplate in privilege-checked news handlers.
///
/// Wraps a handler invocation with privilege checking, automatically cloning
/// the header and handling the async closure boilerplate required by
/// `check_privilege_and_run`.
macro_rules! with_privilege_check {
    ($pool:expr, $session:expr, $header:expr, $privilege:expr, $handler:expr $(, $args:expr)*) => {{
        let reply_header = $header.clone();
        let pool = $pool;
        check_privilege_and_run(
            $session,
            &$header,
            $privilege,
            || async move { Ok($handler(pool, reply_header, $($args),*).await) },
        )
        .await
    }};
}

/// Parameters for retrieving a news article's data.
#[derive(Debug, PartialEq, Eq)]
pub struct ArticleDataRequest {
    pub(crate) path: String,
    pub(crate) article_id: i32,
}

/// Parameters for posting a new news article.
#[derive(Debug, PartialEq, Eq)]
pub struct PostArticleRequest {
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) flags: i32,
    pub(crate) data_flavor: String,
    pub(crate) data: String,
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

enum NewsHandlerError {
    Path(PathLookupError),
    ArticleNotFound,
}

/// Handle news category listing commands after privilege checks.
///
/// # Errors
/// Returns an error if privilege checks or database operations fail.
pub async fn process_category_name_list(
    pool: DbPool,
    session: &Session,
    header: FrameHeader,
    path: Option<String>,
) -> Result<Transaction, CommandError> {
    with_privilege_check!(
        pool,
        session,
        header,
        Privileges::NEWS_READ_ARTICLE,
        handle_category_list,
        path
    )
}

/// Handle news article title listing commands after privilege checks.
///
/// # Errors
/// Returns an error if privilege checks or database operations fail.
pub async fn process_article_name_list(
    pool: DbPool,
    session: &Session,
    header: FrameHeader,
    path: String,
) -> Result<Transaction, CommandError> {
    with_privilege_check!(
        pool,
        session,
        header,
        Privileges::NEWS_READ_ARTICLE,
        handle_article_titles,
        path
    )
}

/// Handle news article data commands after privilege checks.
///
/// # Errors
/// Returns an error if privilege checks or database operations fail.
pub async fn process_article_data(
    pool: DbPool,
    session: &Session,
    header: FrameHeader,
    req: ArticleDataRequest,
) -> Result<Transaction, CommandError> {
    with_privilege_check!(
        pool,
        session,
        header,
        Privileges::NEWS_READ_ARTICLE,
        handle_article_data,
        req
    )
}

/// Handle news article creation commands after privilege checks.
///
/// # Errors
/// Returns an error if privilege checks or database operations fail.
pub async fn process_post_article(
    pool: DbPool,
    session: &Session,
    header: FrameHeader,
    req: PostArticleRequest,
) -> Result<Transaction, CommandError> {
    with_privilege_check!(
        pool,
        session,
        header,
        Privileges::NEWS_POST_ARTICLE,
        handle_post_article,
        req
    )
}

/// Retrieve the list of category names for a given news path.
async fn handle_category_list(
    pool: DbPool,
    header: FrameHeader,
    path: Option<String>,
) -> Transaction {
    handle_list(pool, header, FieldId::NewsCategory, move |conn| {
        Box::pin(async move { list_names_at_path(conn, path.as_deref()).await })
    })
    .await
}

/// Retrieve the titles of articles in a news category.
async fn handle_article_titles(pool: DbPool, header: FrameHeader, path: String) -> Transaction {
    handle_list(pool, header, FieldId::NewsArticle, move |conn| {
        Box::pin(async move { list_article_titles(conn, &path).await })
    })
    .await
}

async fn handle_list<F>(pool: DbPool, header: FrameHeader, field: FieldId, fetch: F) -> Transaction
where
    for<'c> F: FnOnce(&'c mut DbConnection) -> BoxFuture<'c, Result<Vec<String>, PathLookupError>>
        + Send
        + 'static,
{
    run_news_tx(pool, header, move |conn| {
        Box::pin(async move {
            let names = fetch(conn).await.map_err(NewsHandlerError::Path)?;
            let params = names
                .into_iter()
                .map(|name| (field, name.into_bytes()))
                .collect();
            Ok(params)
        })
    })
    .await
}

/// Retrieve a specific news article's data.
async fn handle_article_data(
    pool: DbPool,
    header: FrameHeader,
    req: ArticleDataRequest,
) -> Transaction {
    run_news_tx(pool, header, move |conn| {
        Box::pin(async move {
            let maybe_article = get_article(conn, &req.path, req.article_id)
                .await
                .map_err(NewsHandlerError::Path)?;
            let Some(found_article) = maybe_article else {
                return Err(NewsHandlerError::ArticleNotFound);
            };
            Ok(article_to_params(&found_article))
        })
    })
    .await
}

/// Create a new root article under the provided path.
async fn handle_post_article(
    pool: DbPool,
    header: FrameHeader,
    req: PostArticleRequest,
) -> Transaction {
    run_news_tx(pool, header, move |conn| {
        Box::pin(async move {
            let id = create_root_article(conn, &req.path, req.to_db_params())
                .await
                .map_err(NewsHandlerError::Path)?;
            Ok(vec![(FieldId::NewsArticleId, id.to_be_bytes().to_vec())])
        })
    })
    .await
}

/// Push an optional i32 field as big-endian bytes if present.
fn push_optional_i32(params: &mut Vec<(FieldId, Vec<u8>)>, field: FieldId, value: Option<i32>) {
    if let Some(v) = value {
        params.push((field, v.to_be_bytes().to_vec()));
    }
}

/// Push an optional string field as bytes if present.
fn push_optional_str(params: &mut Vec<(FieldId, Vec<u8>)>, field: FieldId, value: Option<&str>) {
    if let Some(s) = value {
        params.push((field, s.as_bytes().to_vec()));
    }
}

fn article_to_params(article: &Article) -> Vec<(FieldId, Vec<u8>)> {
    let mut params: Vec<(FieldId, Vec<u8>)> = Vec::new();
    params.push((FieldId::NewsTitle, article.title.as_bytes().to_vec()));
    push_optional_str(&mut params, FieldId::NewsPoster, article.poster.as_deref());
    params.push((
        FieldId::NewsDate,
        article
            .posted_at
            .and_utc()
            .timestamp_millis()
            .to_be_bytes()
            .to_vec(),
    ));
    push_optional_i32(&mut params, FieldId::NewsPrevId, article.prev_article_id);
    push_optional_i32(&mut params, FieldId::NewsNextId, article.next_article_id);
    push_optional_i32(
        &mut params,
        FieldId::NewsParentId,
        article.parent_article_id,
    );
    push_optional_i32(
        &mut params,
        FieldId::NewsFirstChildId,
        article.first_child_article_id,
    );
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
    push_optional_str(
        &mut params,
        FieldId::NewsArticleData,
        article.data.as_deref(),
    );
    params
}

/// Helper to execute a news database operation and build a reply transaction.
async fn run_news_tx<F>(pool: DbPool, header: FrameHeader, op: F) -> Transaction
where
    for<'c> F: FnOnce(
            &'c mut DbConnection,
        ) -> BoxFuture<'c, Result<Vec<(FieldId, Vec<u8>)>, NewsHandlerError>>
        + Send
        + 'static,
{
    let result = match pool.get().await {
        Ok(mut conn) => op(&mut conn).await,
        Err(err) => return pool_error_reply(&header, err),
    };
    handle_news_result(&header, result)
}

fn handle_news_result(
    header: &FrameHeader,
    result: Result<Vec<(FieldId, Vec<u8>)>, NewsHandlerError>,
) -> Transaction {
    match result {
        Ok(params) => encode_reply(header, &params),
        Err(err) => news_error_reply(header, err),
    }
}

fn pool_error_reply<E: std::fmt::Display>(header: &FrameHeader, err: E) -> Transaction {
    error!(%err, "failed to get database connection");
    internal_error_reply(header)
}

fn encode_reply(header: &FrameHeader, params: &[(FieldId, Vec<u8>)]) -> Transaction {
    match encode_params(params) {
        Ok(payload) => Transaction {
            header: reply_header(header, 0, payload.len()),
            payload,
        },
        Err(e) => {
            error!(%e, "failed to encode news reply");
            internal_error_reply(header)
        }
    }
}

fn news_error_reply(header: &FrameHeader, err: NewsHandlerError) -> Transaction {
    match err {
        NewsHandlerError::ArticleNotFound => article_not_found_reply(header),
        NewsHandlerError::Path(path_err) => path_error_reply(header, path_err),
    }
}

fn article_not_found_reply(header: &FrameHeader) -> Transaction {
    Transaction {
        header: reply_header(header, NEWS_ERR_ARTICLE_NOT_FOUND, 0),
        payload: Vec::new(),
    }
}

fn path_error_reply(header: &FrameHeader, err: PathLookupError) -> Transaction {
    match err {
        PathLookupError::InvalidPath => {
            tracing::debug!("invalid news path requested");
            unsupported_path_reply(header)
        }
        PathLookupError::Diesel(e) => logged_internal_error(header, "database error", e),
        PathLookupError::Serde(e) => logged_internal_error(header, "serialization error", e),
    }
}

fn unsupported_path_reply(header: &FrameHeader) -> Transaction {
    Transaction {
        header: reply_header(header, NEWS_ERR_PATH_UNSUPPORTED, 0),
        payload: Vec::new(),
    }
}

fn logged_internal_error<E: std::fmt::Display>(
    header: &FrameHeader,
    context: &str,
    err: E,
) -> Transaction {
    error!(%err, context, "news handler error");
    internal_error_reply(header)
}

fn internal_error_reply(header: &FrameHeader) -> Transaction {
    Transaction {
        header: reply_header(header, ERR_INTERNAL_SERVER, 0),
        payload: Vec::new(),
    }
}
#[cfg(test)]
mod tests;
