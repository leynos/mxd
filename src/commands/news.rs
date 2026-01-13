//! News command helpers and database operations.
//!
//! These helpers keep news-related transactions and database access logic
//! grouped together for reuse by command processing.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use futures_util::future::BoxFuture;
use tracing::error;

use super::{ERR_INTERNAL_SERVER, NEWS_ERR_PATH_UNSUPPORTED};
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
    header_util::reply_header,
    transaction::{FrameHeader, Transaction, encode_params},
};

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
pub(super) async fn handle_category_list(
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
pub(super) async fn handle_article_titles(
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
pub(super) async fn handle_article_data(
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

/// Parameters for retrieving a news article's data.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct GetArticleDataRequest {
    pub(super) path: String,
    pub(super) article_id: i32,
}

/// Parameters for posting a new news article.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct PostArticleRequest {
    pub(super) path: String,
    pub(super) title: String,
    pub(super) flags: i32,
    pub(super) data_flavor: String,
    pub(super) data: String,
}

impl PostArticleRequest {
    /// Build database parameters from this request.
    ///
    /// The returned [`CreateRootArticleParams`] borrows from `self` and contains
    /// all fields except `path`, which is used separately for path lookup.
    pub(super) fn to_db_params(&self) -> CreateRootArticleParams<'_> {
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
pub(super) async fn handle_post_article(
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

#[cfg(test)]
mod tests {
    use super::*;

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
