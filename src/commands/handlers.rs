//! Command execution handlers and shared helpers.
//!
//! This module implements the per-command processing logic invoked by
//! `Command::process` and centralises reply construction shared across handlers.

use std::net::SocketAddr;

use super::{
    Command,
    CommandError,
    ERR_INTERNAL_SERVER,
    ERR_INVALID_PAYLOAD,
    HandlerContext,
    check_privilege_and_run,
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
    ) -> Result<Transaction, CommandError> {
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
    ) -> Result<Transaction, CommandError> {
        let HandlerContext {
            pool,
            session,
            header,
        } = ctx;
        let header_reply = header.clone();
        // Extract user_id early; check_privilege_and_run also validates auth,
        // so this provides the value for the closure while maintaining a single
        // auth check path.
        let Some(user_id) = session.user_id else {
            return Ok(privilege_error_reply(
                &header,
                PrivilegeError::NotAuthenticated,
            ));
        };
        check_privilege_and_run(session, &header, Privileges::DOWNLOAD_FILE, || async move {
            let mut conn = pool.get().await?;
            let files = crate::db::list_files_for_user(&mut conn, user_id).await?;
            let params: Vec<(FieldId, &[u8])> = files
                .iter()
                .map(|f| (FieldId::FileName, f.name.as_bytes()))
                .collect();
            let payload = encode_params(&params)?;
            Ok(Transaction {
                header: reply_header(&header_reply, 0, payload.len()),
                payload,
            })
        })
        .await
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
    tracing::warn!(%peer, ty = %header.ty, "unknown transaction");
    Transaction {
        header: reply_header(header, ERR_INTERNAL_SERVER, 0),
        payload: Vec::new(),
    }
}
