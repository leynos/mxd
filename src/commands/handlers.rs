//! Command execution handlers and shared helpers.
//!
//! This module implements the per-command processing logic invoked by
//! `Command::process` and centralizes reply construction shared across handlers.

use std::net::SocketAddr;

use tokio::time::{Duration, sleep};
use tracing::warn;

use super::{
    Command,
    CommandContext,
    CommandError,
    ERR_INTERNAL_SERVER,
    ERR_INVALID_PAYLOAD,
    UserInfoUpdate,
    check_privilege_and_run,
    privilege_error_reply,
};
use crate::{
    db::{DbPool, get_user_by_id},
    field_id::FieldId,
    handler::PrivilegeError,
    header_util::reply_header,
    login::{LoginRequest, handle_login},
    presence::{
        PresenceRegistry,
        build_client_info_text_reply,
        build_notify_change_user,
        build_user_name_list_reply,
    },
    privileges::Privileges,
    server::outbound::{OutboundMessaging, OutboundPriority, OutboundTarget, OutboundTransport},
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
                let Some(uid) = session_ref.user_id else {
                    tracing::error!("authenticated session missing user id in file list handler");
                    return Err(CommandError::Invariant(
                        "authenticated session missing user id",
                    ));
                };
                let mut conn = pool.get().await?;
                let files =
                    crate::db::list_visible_root_file_nodes_for_user(&mut conn, uid).await?;
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

    pub(super) async fn process_login_with_presence(
        context: CommandContext<'_>,
        req: LoginRequest,
    ) -> Result<(), CommandError> {
        let CommandContext {
            peer,
            pool,
            session,
            transport,
            messaging,
            presence,
            presence_connection_id,
        } = context;
        let presence_context = PresenceContext {
            transport,
            messaging,
            presence,
        };
        let reply = handle_login(peer, session, pool, req).await?;
        presence_context.transport.send_reply(reply)?;
        let Some(connection_id) = presence_connection_id else {
            return Ok(());
        };
        let Some(snapshot) = session.presence_snapshot(connection_id) else {
            return Ok(());
        };
        build_notify_change_user(&snapshot)?;
        let upsert = presence_context.presence.upsert(snapshot)?;
        if upsert.peer_ids.is_empty() {
            return Ok(());
        }
        let notification = build_notify_change_user(&upsert.snapshot)?;
        push_with_retry_to_peers(presence_context.messaging, &upsert.peer_ids, notification).await;
        Ok(())
    }

    pub(super) fn process_get_user_name_list(
        context: CommandContext<'_>,
        header: &FrameHeader,
    ) -> Result<(), CommandError> {
        let CommandContext {
            session,
            transport,
            presence,
            ..
        } = context;
        if !session.is_online() {
            transport.send_reply(privilege_error_reply(
                header,
                PrivilegeError::NotAuthenticated,
            ))?;
            return Ok(());
        }
        let reply = build_user_name_list_reply(header, &presence.online_snapshots())?;
        transport.send_reply(reply)?;
        Ok(())
    }

    pub(super) async fn process_get_client_info_text(
        context: CommandContext<'_>,
        header: FrameHeader,
        target_user_id: i32,
    ) -> Result<(), CommandError> {
        let CommandContext {
            pool,
            session,
            transport,
            presence,
            ..
        } = context;
        let header_reply = header.clone();
        let reply = check_privilege_and_run(
            session,
            &header,
            Privileges::GET_CLIENT_INFO,
            || async move {
                if let Some(snapshot) = presence.snapshot_for_user_id(target_user_id) {
                    return build_client_info_text_reply(&header_reply, &snapshot.display_name, "")
                        .map_err(CommandError::from);
                }
                let mut conn = pool.get().await?;
                match get_user_by_id(&mut conn, target_user_id).await? {
                    Some(user) => build_client_info_text_reply(&header_reply, &user.username, "")
                        .map_err(CommandError::from),
                    None => Ok(Transaction {
                        header: reply_header(&header_reply, ERR_INTERNAL_SERVER, 0),
                        payload: Vec::new(),
                    }),
                }
            },
        )
        .await?;
        transport.send_reply(reply)?;
        Ok(())
    }

    pub(super) async fn process_set_client_user_info(
        context: CommandContext<'_>,
        header: FrameHeader,
        update: UserInfoUpdate,
    ) -> Result<(), CommandError> {
        let CommandContext {
            session,
            transport,
            messaging,
            presence,
            presence_connection_id,
            ..
        } = context;
        let presence_context = PresenceContext {
            transport,
            messaging,
            presence,
        };
        if let Err(error) = session.require_authenticated() {
            presence_context
                .transport
                .send_reply(privilege_error_reply(&header, error))?;
            return Ok(());
        }

        apply_user_info_update(session, update);
        let maybe_snapshot = presence_connection_id
            .and_then(|connection_id| session.presence_snapshot(connection_id));
        if let Some(candidate_snapshot) = &maybe_snapshot {
            build_notify_change_user(candidate_snapshot)?;
        }
        presence_context
            .transport
            .send_reply(empty_success_reply(&header))?;

        let Some(snapshot) = maybe_snapshot else {
            return Ok(());
        };
        let upsert = presence_context.presence.upsert(snapshot)?;
        if upsert.peer_ids.is_empty() {
            return Ok(());
        }
        let notification = build_notify_change_user(&upsert.snapshot)?;
        push_with_retry_to_peers(presence_context.messaging, &upsert.peer_ids, notification).await;
        Ok(())
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

fn empty_success_reply(header: &FrameHeader) -> Transaction {
    Transaction {
        header: reply_header(header, 0, 0),
        payload: Vec::new(),
    }
}

fn apply_user_info_update(session: &mut crate::handler::Session, update: UserInfoUpdate) {
    if let Some(display_name) = update.display_name {
        session.display_name = display_name;
    }
    if let Some(icon_id) = update.icon_id {
        session.icon_id = icon_id;
    }
    let final_flags = update.options.unwrap_or(session.connection_flags);
    if let Some(options) = update.options {
        session.connection_flags = options;
    }
    if final_flags.has_auto_response() {
        if let Some(auto_response) = update.auto_response {
            session.auto_response = Some(auto_response);
        }
    } else {
        session.auto_response = None;
    }
}

async fn push_with_retry_to_peers(
    messaging: &dyn OutboundMessaging,
    connection_ids: &[crate::server::outbound::OutboundConnectionId],
    message: Transaction,
) {
    for &connection_id in connection_ids {
        push_with_retry_to_peer(messaging, connection_id, message.clone()).await;
    }
}

async fn push_with_retry_to_peer(
    messaging: &dyn OutboundMessaging,
    connection_id: crate::server::outbound::OutboundConnectionId,
    message: Transaction,
) {
    if let Err(error) = push_with_retry(messaging, connection_id, message).await {
        warn!(
            ?error,
            target = connection_id.as_u64(),
            "presence notification delivery failed"
        );
    }
}
async fn push_with_retry_to_peer(
    messaging: &dyn OutboundMessaging,
    connection_id: crate::server::outbound::OutboundConnectionId,
    message: Transaction,
) {
    if let Err(error) = push_with_retry(messaging, connection_id, message).await {
        warn!(
            ?error,
            target = connection_id.as_u64(),
            "presence notification delivery failed"
        );
    }
}
async fn push_with_retry(
    messaging: &dyn OutboundMessaging,
    connection_id: crate::server::outbound::OutboundConnectionId,
    message: Transaction,
) -> Result<(), crate::server::outbound::OutboundError> {
    for attempt in 0..PRESENCE_PUSH_RETRY_ATTEMPTS {
        match messaging
            .push(
                OutboundTarget::Connection(connection_id),
                message.clone(),
                OutboundPriority::High,
            )
            .await
        {
            Ok(()) => return Ok(()),
            Err(crate::server::outbound::OutboundError::TargetUnavailable)
                if attempt + 1 < PRESENCE_PUSH_RETRY_ATTEMPTS =>
            {
                sleep(PRESENCE_PUSH_RETRY_DELAY).await;
            }
            Err(error) => return Err(error),
        }
    }

    Err(crate::server::outbound::OutboundError::TargetUnavailable)
}

pub(super) struct PresenceContext<'a> {
    pub(super) transport: &'a mut dyn OutboundTransport,
    pub(super) messaging: &'a dyn OutboundMessaging,
    pub(super) presence: &'a PresenceRegistry,
}


const PRESENCE_PUSH_RETRY_ATTEMPTS: usize = 5;

const PRESENCE_PUSH_RETRY_DELAY: Duration = Duration::from_millis(50);
