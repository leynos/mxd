//! Shared helpers for wireframe routing tests.

use std::net::SocketAddr;

#[cfg(feature = "sqlite")]
use tempfile::TempDir;
use test_util::AnyError;
#[cfg(feature = "postgres")]
use test_util::postgres::PostgresTestDb;
use tokio::runtime::Runtime;

use crate::{
    db::{DbPool, establish_pool},
    field_id::FieldId,
    handler::Session,
    transaction::{FrameHeader, Transaction, decode_params, encode_params, parse_transaction},
    transaction_type::TransactionType,
    wireframe::{routes::process_transaction_bytes, test_helpers::transaction_bytes},
};

/// Fixture database setup function signature.
pub(super) type SetupFn = fn(&str) -> Result<(), AnyError>;

pub(super) struct TestDb {
    pool: DbPool,
    #[cfg(feature = "sqlite")]
    _temp_dir: TempDir,
    #[cfg(feature = "postgres")]
    _postgres: PostgresTestDb,
}

impl TestDb {
    pub(super) fn pool(&self) -> DbPool { self.pool.clone() }
}

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
pub(super) fn runtime() -> Runtime { Runtime::new().expect("runtime") }

fn build_frame(
    ty: TransactionType,
    id: u32,
    params: &[(FieldId, &[u8])],
) -> Result<Vec<u8>, AnyError> {
    let payload = if params.is_empty() {
        Vec::new()
    } else {
        encode_params(params)?
    };
    let payload_size = u32::try_from(payload.len())?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: ty.into(),
        id,
        error: 0,
        total_size: payload_size,
        data_size: payload_size,
    };
    Ok(transaction_bytes(&header, &payload))
}

pub(super) fn decode_reply_params(tx: &Transaction) -> Result<Vec<(FieldId, Vec<u8>)>, AnyError> {
    Ok(decode_params(&tx.payload)?)
}

pub(super) fn collect_strings(
    params: &[(FieldId, Vec<u8>)],
    field_id: FieldId,
) -> Result<Vec<String>, AnyError> {
    params
        .iter()
        .filter(|(id, _)| id == &field_id)
        .map(|(_, data)| String::from_utf8(data.clone()).map_err(|e| -> AnyError { e.into() }))
        .collect()
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

pub(super) fn build_test_db(rt: &Runtime, setup: SetupFn) -> Result<Option<TestDb>, AnyError> {
    #[cfg(feature = "sqlite")]
    {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path().join("mxd.db");
        let db_url = path
            .to_str()
            .ok_or_else(|| "database path is not valid UTF-8".to_owned())?;
        setup(db_url)?;
        let pool = rt.block_on(establish_pool(db_url))?;
        Ok(Some(TestDb {
            pool,
            _temp_dir: temp_dir,
        }))
    }

    #[cfg(feature = "postgres")]
    {
        let db = match PostgresTestDb::new() {
            Ok(db) => db,
            Err(err) if err.is_unavailable() => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let db_url = db.url.as_ref();
        setup(db_url)?;
        let pool = rt.block_on(establish_pool(db_url))?;
        Ok(Some(TestDb {
            pool,
            _postgres: db,
        }))
    }

    #[cfg(not(any(feature = "sqlite", feature = "postgres")))]
    {
        let _ = (rt, setup);
        Ok(None)
    }
}
