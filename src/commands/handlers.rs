//! Command execution handlers and shared helpers.
//!
//! This module implements the per-command processing logic invoked by
//! `Command::process` and centralizes reply construction shared across handlers.

use std::net::SocketAddr;

use super::{
    Command,
    CommandError,
    ERR_INTERNAL_SERVER,
    ERR_INVALID_PAYLOAD,
    check_privilege_and_run,
};
use crate::{
    db::DbPool,
    field_id::FieldId,
    header_util::reply_header,
    login::{LoginRequest, handle_login},
    privileges::Privileges,
    transaction::{FrameHeader, Transaction, encode_params},
};

impl Command {
    pub(super) async fn process_login(
        peer: SocketAddr,
        pool: DbPool,
        session: &mut crate::handler::Session,
        req: LoginRequest,
    ) -> Result<Transaction, CommandError> {
        handle_login(peer, session, pool, req).await
    }

    pub(super) async fn process_get_file_name_list(
        pool: DbPool,
        session: &mut crate::handler::Session,
        header: FrameHeader,
    ) -> Result<Transaction, CommandError> {
        let header_reply = header.clone();
        let session_ref = &*session;
        check_privilege_and_run(
            session_ref,
            &header,
            Privileges::DOWNLOAD_FILE,
            || async move {
                // Invariant: require_privilege guarantees authentication; read user_id after the
                // check. Defensive fallback guards the invariant without expect.
                let Some(uid) = session_ref.user_id else {
                    return Ok(Transaction {
                        header: reply_header(&header_reply, ERR_INTERNAL_SERVER, 0),
                        payload: Vec::new(),
                    });
                };
                let mut conn = pool.get().await?;
                let files = crate::db::list_files_for_user(&mut conn, uid).await?;
                let params: Vec<(FieldId, &[u8])> = files
                    .iter()
                    .map(|f| (FieldId::FileName, f.name.as_bytes()))
                    .collect();
                let payload = encode_params(&params)?;
                Ok(Transaction {
                    header: reply_header(&header_reply, 0, payload.len()),
                    payload,
                })
            },
        )
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
