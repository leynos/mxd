//! Shared helpers for wireframe routing tests.

use std::net::SocketAddr;

use test_util::AnyError;
use tokio::runtime::{Builder, Runtime};

pub(super) use crate::wireframe::test_helpers::{build_frame, collect_strings};
use crate::{
    db::DbPool,
    field_id::FieldId,
    handler::Session,
    privileges::Privileges,
    server::outbound::NoopOutboundMessaging,
    transaction::{Transaction, decode_params, parse_transaction},
    transaction_type::TransactionType,
    wireframe::routes::{RouteContext, process_transaction_bytes},
};

/// Test harness context that bundles routing state for wireframe handlers.
pub(super) struct RouteTestContext {
    /// Database pool passed into routing so handlers can query state.
    pool: DbPool,
    /// Session state mutated by routing; stored by value and reused across calls.
    pub(super) session: Session,
    /// Peer socket address supplied to routing for auditing and auth checks.
    peer: SocketAddr,
}

impl RouteTestContext {
    /// Create a new routing context with the provided database pool.
    ///
    /// # Parameters
    ///
    /// * `pool` - Database pool used by routing handlers.
    ///
    /// # Errors
    ///
    /// Returns an error if the fixed peer address literal fails to parse.
    pub(super) fn new(pool: DbPool) -> Result<Self, AnyError> {
        let peer = "127.0.0.1:12345".parse()?;
        Ok(Self {
            pool,
            session: Session::default(),
            peer,
        })
    }

    /// Authenticate the session with default user privileges.
    ///
    /// Sets the session's user ID and grants default user privileges so that
    /// handlers requiring authentication will succeed. Use this before testing
    /// privileged operations without going through the full login flow.
    pub(super) fn authenticate(&mut self, user_id: i32) {
        self.session.user_id = Some(user_id);
        self.session.privileges = Privileges::default_user();
    }

    /// Authenticate with custom privileges.
    ///
    /// Sets the session's user ID and grants the specified privileges.
    pub(super) fn authenticate_with_privileges(&mut self, user_id: i32, privileges: Privileges) {
        self.session.user_id = Some(user_id);
        self.session.privileges = privileges;
    }

    /// Send a transaction through routing and parse the reply.
    ///
    /// # Parameters
    ///
    /// * `ty` - Transaction type to encode and route.
    /// * `id` - Transaction identifier preserved in the reply.
    /// * `params` - Encoded parameters to attach to the request.
    ///
    /// # Errors
    ///
    /// Returns an error if frame encoding, route processing, or reply parsing
    /// fails.
    pub(super) async fn send(
        &mut self,
        ty: TransactionType,
        id: u32,
        params: &[(FieldId, &[u8])],
    ) -> Result<Transaction, AnyError> {
        let frame = build_frame(ty, id, params)?;
        let messaging = NoopOutboundMessaging;
        let reply = process_transaction_bytes(
            &frame,
            RouteContext {
                peer: self.peer,
                pool: self.pool.clone(),
                session: &mut self.session,
                messaging: &messaging,
            },
        )
        .await;
        Ok(parse_transaction(&reply)?)
    }
}

/// Build a single-threaded Tokio runtime with all features enabled.
///
/// # Errors
///
/// Returns an error if runtime construction fails.
pub(super) fn runtime() -> Result<Runtime, AnyError> {
    Ok(Builder::new_current_thread().enable_all().build()?)
}

/// Decode reply payload bytes into parameter tuples.
///
/// # Parameters
///
/// * `tx` - Reply transaction whose payload contains encoded parameters.
///
/// # Errors
///
/// Returns an error if parameter decoding fails.
pub(super) fn decode_reply_params(tx: &Transaction) -> Result<Vec<(FieldId, Vec<u8>)>, AnyError> {
    Ok(decode_params(&tx.payload)?)
}

/// Locate a field in parameters and decode it as a UTF-8 string.
///
/// # Parameters
///
/// * `params` - Decoded parameter list to search.
/// * `field_id` - Field identifier to locate.
///
/// # Errors
///
/// Returns an error if the field is missing or contains invalid UTF-8.
pub(super) fn find_string(
    params: &[(FieldId, Vec<u8>)],
    field_id: FieldId,
) -> Result<String, AnyError> {
    let data = params
        .iter()
        .find(|(id, _)| id == &field_id)
        .map(|(_, data)| data.as_slice())
        .ok_or_else(|| anyhow::anyhow!("missing {field_id:?} field"))?;
    let text = std::str::from_utf8(data)?;
    Ok(text.to_owned())
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
/// Locate a field in parameters and decode it as a big-endian `i32`.
///
/// # Parameters
///
/// * `params` - Decoded parameter list to search.
/// * `field_id` - Field identifier to locate.
///
/// # Errors
///
/// Returns an error if the field is missing or the payload is not a 4-byte
/// big-endian integer.
pub(super) fn find_i32(params: &[(FieldId, Vec<u8>)], field_id: FieldId) -> Result<i32, AnyError> {
    let bytes = params
        .iter()
        .find(|(id, _)| id == &field_id)
        .map(|(_, data)| data.as_slice())
        .ok_or_else(|| anyhow::anyhow!("missing {field_id:?} field"))?;
    let raw: [u8; 4] = bytes.try_into()?;
    Ok(i32::from_be_bytes(raw))
}
