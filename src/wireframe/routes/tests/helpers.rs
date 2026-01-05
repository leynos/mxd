//! Shared helpers for wireframe routing tests.

use std::net::SocketAddr;

use test_util::AnyError;
use tokio::runtime::{Builder, Runtime};

pub(super) use crate::wireframe::test_helpers::{build_frame, collect_strings};
use crate::{
    db::DbPool,
    field_id::FieldId,
    handler::Session,
    transaction::{Transaction, decode_params, parse_transaction},
    transaction_type::TransactionType,
    wireframe::routes::process_transaction_bytes,
};

pub(super) struct RouteTestContext {
    pool: DbPool,
    pub(super) session: Session,
    peer: SocketAddr,
}

impl RouteTestContext {
    #[expect(clippy::expect_used, reason = "test setup")]
    pub(super) fn new(pool: DbPool) -> Self {
        let peer = "127.0.0.1:12345".parse().expect("valid peer addr");
        Self {
            pool,
            session: Session::default(),
            peer,
        }
    }

    pub(super) async fn send(
        &mut self,
        ty: TransactionType,
        id: u32,
        params: &[(FieldId, &[u8])],
    ) -> Result<Transaction, AnyError> {
        let frame = build_frame(ty, id, params)?;
        let reply =
            process_transaction_bytes(&frame, self.peer, self.pool.clone(), &mut self.session)
                .await;
        Ok(parse_transaction(&reply)?)
    }
}

#[expect(clippy::expect_used, reason = "test runtime setup")]
pub(super) fn runtime() -> Runtime {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

pub(super) fn decode_reply_params(tx: &Transaction) -> Result<Vec<(FieldId, Vec<u8>)>, AnyError> {
    Ok(decode_params(&tx.payload)?)
}

pub(super) fn find_string(
    params: &[(FieldId, Vec<u8>)],
    field_id: FieldId,
) -> Result<String, AnyError> {
    let data = params
        .iter()
        .find(|(id, _)| id == &field_id)
        .map(|(_, data)| data)
        .ok_or_else(|| -> AnyError { format!("missing {field_id:?} field").into() })?;
    Ok(String::from_utf8(data.clone())?)
}

#[expect(clippy::big_endian_bytes, reason = "network protocol")]
pub(super) fn find_i32(params: &[(FieldId, Vec<u8>)], field_id: FieldId) -> Result<i32, AnyError> {
    let bytes = params
        .iter()
        .find(|(id, _)| id == &field_id)
        .map(|(_, data)| data.as_slice())
        .ok_or_else(|| -> AnyError { format!("missing {field_id:?} field").into() })?;
    let raw: [u8; 4] = bytes.try_into()?;
    Ok(i32::from_be_bytes(raw))
}
