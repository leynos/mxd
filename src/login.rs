//! Implementation of the login transaction.
//!
//! Validates user credentials against the database and updates session state
//! on success. Login attempts are logged and rejected with appropriate error
//! codes when validation fails.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
#![expect(
    clippy::cognitive_complexity,
    reason = "login flow requires multiple validation steps"
)]

use std::net::SocketAddr;

use tracing::{info, warn};

use crate::{
    db::{DbPool, get_user_by_name},
    field_id::FieldId,
    header_util::reply_header,
    transaction::{FrameHeader, Transaction, encode_params},
    users::verify_password,
};

/// Parameters for a login request containing credentials and protocol header.
pub(crate) struct LoginRequest {
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) header: FrameHeader,
}

/// Handle a user login request.
///
/// # Errors
/// Returns an error if database access fails or credentials are invalid.
#[must_use = "handle the result"]
pub(crate) async fn handle_login(
    peer: SocketAddr,
    session: &mut crate::handler::Session,
    pool: DbPool,
    req: LoginRequest,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let mut conn = pool.get().await?;
    let user = get_user_by_name(&mut conn, &req.username).await?;
    let (error, payload) = if let Some(u) = user {
        if verify_password(&u.password, &req.password) {
            session.user_id = Some(u.id);
            let params = encode_params(&[(
                FieldId::Version,
                &crate::protocol::CLIENT_VERSION.to_be_bytes(),
            )])?;
            (0u32, params)
        } else {
            (1u32, Vec::new())
        }
    } else {
        (1u32, Vec::new())
    };
    let reply = Transaction {
        header: reply_header(&req.header, error, payload.len()),
        payload,
    };
    if error == 0 {
        info!(%peer, username = %req.username, "authenticated");
    } else {
        warn!(%peer, username = %req.username, "authentication failed");
    }
    Ok(reply)
}
