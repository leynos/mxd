//! Command execution handlers and shared helpers.

use std::net::SocketAddr;

use super::{
    Command,
    ERR_INSUFFICIENT_PRIVILEGES,
    ERR_INTERNAL_SERVER,
    ERR_INVALID_PAYLOAD,
    ERR_NOT_AUTHENTICATED,
    news::{
        PostArticleRequest,
        handle_article_data,
        handle_article_titles,
        handle_category_list,
        handle_post_article,
    },
};
use crate::{
    db::DbPool,
    field_id::FieldId,
    handler::PrivilegeError,
    header_util::reply_header,
    login::{LoginRequest, handle_login},
    privileges::Privileges,
    transaction::{FrameHeader, Transaction, encode_params},
};

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

impl Command {
    #[expect(
        clippy::too_many_arguments,
        reason = "command fields map directly to handler arguments"
    )]
    pub(super) async fn process_login(
        peer: SocketAddr,
        pool: DbPool,
        session: &mut crate::handler::Session,
        username: String,
        password: String,
        header: FrameHeader,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let req = LoginRequest {
            username,
            password,
            header,
        };
        handle_login(peer, session, pool, req).await
    }

    pub(super) async fn process_get_file_name_list(
        pool: DbPool,
        session: &mut crate::handler::Session,
        header: FrameHeader,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
        if let Err(e) = session.require_privilege(Privileges::DOWNLOAD_FILE) {
            return Ok(privilege_error_reply(&header, e));
        }
        let user_id = session.user_id.ok_or(PrivilegeError::NotAuthenticated)?;
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

    pub(super) async fn process_get_news_category_name_list(
        pool: DbPool,
        session: &mut crate::handler::Session,
        header: FrameHeader,
        path: Option<String>,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
        if let Err(e) = session.require_privilege(Privileges::NEWS_READ_ARTICLE) {
            return Ok(privilege_error_reply(&header, e));
        }
        handle_category_list(pool, header, path).await
    }

    pub(super) async fn process_get_news_article_name_list(
        pool: DbPool,
        session: &mut crate::handler::Session,
        header: FrameHeader,
        path: String,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
        if let Err(e) = session.require_privilege(Privileges::NEWS_READ_ARTICLE) {
            return Ok(privilege_error_reply(&header, e));
        }
        handle_article_titles(pool, header, path).await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "command fields map directly to handler arguments"
    )]
    pub(super) async fn process_get_news_article_data(
        pool: DbPool,
        session: &mut crate::handler::Session,
        header: FrameHeader,
        path: String,
        article_id: i32,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
        if let Err(e) = session.require_privilege(Privileges::NEWS_READ_ARTICLE) {
            return Ok(privilege_error_reply(&header, e));
        }
        handle_article_data(pool, header, path, article_id).await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "command fields map directly to handler arguments"
    )]
    pub(super) async fn process_post_news_article(
        pool: DbPool,
        session: &mut crate::handler::Session,
        header: FrameHeader,
        path: String,
        title: String,
        flags: i32,
        data_flavor: String,
        data: String,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
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

    #[expect(
        clippy::needless_pass_by_value,
        reason = "signature required by Command.process dispatch"
    )]
    pub(super) fn process_invalid_payload(header: FrameHeader) -> Transaction {
        Transaction {
            header: reply_header(&header, ERR_INVALID_PAYLOAD, 0),
            payload: Vec::new(),
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "signature required by Command.process dispatch"
    )]
    pub(super) fn process_unknown(peer: SocketAddr, header: FrameHeader) -> Transaction {
        handle_unknown(peer, &header)
    }
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
