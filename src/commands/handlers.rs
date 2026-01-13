//! Command execution handlers and shared helpers.

use std::net::SocketAddr;

use super::{
    Command,
    ERR_INTERNAL_SERVER,
    ERR_INVALID_PAYLOAD,
    HandlerContext,
    privilege_error_reply,
};
use crate::{
    field_id::FieldId,
    handler::PrivilegeError,
    header_util::reply_header,
    login::{LoginRequest, handle_login},
    privileges::Privileges,
    transaction::{FrameHeader, Transaction, encode_params},
};

impl Command {
    pub(super) async fn process_login(
        peer: SocketAddr,
        ctx: HandlerContext<'_>,
        username: String,
        password: String,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let HandlerContext {
            pool,
            session,
            header,
        } = ctx;
        let req = LoginRequest {
            username,
            password,
            header,
        };
        handle_login(peer, session, pool, req).await
    }

    pub(super) async fn process_get_file_name_list(
        ctx: HandlerContext<'_>,
    ) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let HandlerContext {
            pool,
            session,
            header,
        } = ctx;
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
