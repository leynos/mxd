use std::net::SocketAddr;

use tracing::{info, warn};

use crate::{
    db::{DbPool, get_user_by_name},
    field_id::FieldId,
    header_util::reply_header,
    transaction::{FrameHeader, Transaction, encode_params},
    users::verify_password,
};

/// Handle a user login request.
///
/// # Errors
/// Returns an error if database access fails or credentials are invalid.
#[must_use = "handle the result"]
pub async fn handle_login(
    peer: SocketAddr,
    session: &mut crate::handler::Session,
    pool: DbPool,
    username: String,
    password: String,
    header: FrameHeader,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let mut conn = pool.get().await?;
    let user = get_user_by_name(&mut conn, &username).await?;
    let (error, payload) = if let Some(u) = user {
        if verify_password(&u.password, &password) {
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
        header: reply_header(&header, error, payload.len()),
        payload,
    };
    if error == 0 {
        info!(%peer, %username, "authenticated");
    } else {
        warn!(%peer, %username, "authentication failed");
    }
    Ok(reply)
}
