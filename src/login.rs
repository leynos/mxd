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
    commands::CommandError,
    db::{DbPool, get_user_by_name},
    field_id::FieldId,
    header_util::reply_header,
    privileges::Privileges,
    transaction::{FrameHeader, Transaction, encode_params},
    users::verify_password,
};

/// Parameters for a login request containing credentials and protocol header.
#[derive(Debug)]
pub struct LoginRequest {
    /// Username for authentication.
    pub username: String,
    /// Password for authentication.
    pub password: String,
    /// Transaction frame header.
    pub header: FrameHeader,
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
) -> Result<Transaction, CommandError> {
    let mut conn = pool.get().await?;
    let user = get_user_by_name(&mut conn, &req.username).await?;
    let (error, payload) = if let Some(u) = user {
        if verify_password(&u.password, &req.password) {
            session.user_id = Some(u.id);
            // Grant default user privileges on successful authentication.
            // TODO(task 5.1): Load privileges from user account in database.
            session.privileges = Privileges::default_user();
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

#[cfg(test)]
mod tests {
    //! Behavioural coverage for login edge cases.

    use std::net::SocketAddr;

    use anyhow::anyhow;
    use test_util::{AnyError, DatabaseUrl, build_test_db, with_db};
    use tokio::runtime::Runtime;

    use super::{LoginRequest, handle_login};
    use crate::{
        db::create_user,
        handler::Session,
        models::NewUser,
        transaction::FrameHeader,
        transaction_type::TransactionType,
    };

    fn setup_user_with_invalid_hash(db: DatabaseUrl) -> Result<(), AnyError> {
        with_db(db, |conn| {
            Box::pin(async move {
                let new_user = NewUser {
                    username: "alice",
                    password: "not-a-valid-hash",
                };
                create_user(conn, &new_user).await?;
                Ok(())
            })
        })
    }

    #[test]
    fn handle_login_rejects_invalid_password_hashes() -> Result<(), AnyError> {
        let rt = Runtime::new()?;
        let Some(db) = build_test_db(&rt, setup_user_with_invalid_hash)? else {
            return Ok(());
        };
        let mut session = Session::default();
        let peer: SocketAddr = "127.0.0.1:12345".parse()?;
        let req = LoginRequest {
            username: "alice".to_string(),
            password: "secret".to_string(),
            header: FrameHeader {
                flags: 0,
                is_reply: 0,
                ty: TransactionType::Login.into(),
                id: 1,
                error: 0,
                total_size: 0,
                data_size: 0,
            },
        };

        let reply = rt.block_on(handle_login(peer, &mut session, db.pool(), req))?;

        if reply.header.error != 1 {
            return Err(anyhow!(
                "expected error code 1 for invalid hash, got {}",
                reply.header.error
            ));
        }
        if session.user_id.is_some() {
            return Err(anyhow!("session should remain unauthenticated"));
        }
        Ok(())
    }
}
