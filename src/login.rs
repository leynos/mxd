use std::net::SocketAddr;

use crate::db::{DbPool, get_user_by_name};
use crate::field_id::FieldId;
use crate::transaction::{FrameHeader, Transaction, encode_params};
use crate::users::verify_password;

/// Construct a reply header mirroring the request header fields and setting the
/// payload size and error code.
fn reply_header(req: &FrameHeader, payload_error: u32, payload_len: usize) -> FrameHeader {
    FrameHeader {
        flags: 0,
        is_reply: 1,
        ty: req.ty,
        id: req.id,
        error: payload_error,
        total_size: payload_len as u32,
        data_size: payload_len as u32,
    }
}

/// Handle a user login request.
pub async fn handle_login(
    peer: SocketAddr,
    session: &mut crate::handler::Session,
    pool: DbPool,
    username: String,
    password: String,
    header: FrameHeader,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let mut conn = pool.get().await?;
    let user = get_user_by_name(&mut conn, &username).await?;
    let (error, payload) = if let Some(u) = user {
        if verify_password(&u.password, &password) {
            session.user_id = Some(u.id);
            let params = encode_params(&[(
                FieldId::Version,
                &crate::protocol::CLIENT_VERSION.to_be_bytes(),
            )]);
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
        println!("{peer} authenticated as {username}");
    }
    Ok(reply)
}
