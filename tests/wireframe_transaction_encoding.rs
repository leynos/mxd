#![expect(clippy::expect_used, reason = "test assertions")]

//! Behavioural tests for wireframe transaction encoding parity.

use std::cell::RefCell;

use bincode::{config, encode_to_vec};
use mxd::{
    field_id::FieldId,
    transaction::{FrameHeader, MAX_PAYLOAD_SIZE, Transaction, TransactionWriter, encode_params},
    wireframe::codec::HotlineTransaction,
};
use rstest::fixture;
use rstest_bdd::{assert_step_err, assert_step_ok};
use rstest_bdd_macros::{given, scenario, then, when};
use tokio::{io::AsyncReadExt, runtime::Runtime};

fn hotline_config() -> impl bincode::config::Config {
    config::standard()
        .with_big_endian()
        .with_fixed_int_encoding()
}

struct EncodingWorld {
    params: RefCell<Vec<(FieldId, Vec<u8>)>>,
    transaction: RefCell<Option<Transaction>>,
    outcome: RefCell<Option<Result<EncodingResult, String>>>,
    rt: Runtime,
}

struct EncodingResult {
    wireframe_bytes: Vec<u8>,
    legacy_bytes: Vec<u8>,
}

impl EncodingWorld {
    fn new() -> Self {
        let rt = Runtime::new().expect("runtime");
        Self {
            params: RefCell::new(Vec::new()),
            transaction: RefCell::new(None),
            outcome: RefCell::new(None),
            rt,
        }
    }

    fn set_params(&self, params: Vec<(FieldId, Vec<u8>)>) {
        *self.transaction.borrow_mut() = None;
        *self.params.borrow_mut() = params;
    }

    fn set_transaction(&self, tx: Transaction) {
        self.params.borrow_mut().clear();
        *self.transaction.borrow_mut() = Some(tx);
    }

    fn encode(&self) {
        let params = self.params.borrow().clone();
        let maybe_tx = self.transaction.borrow().clone();
        let result: Result<EncodingResult, String> = maybe_tx.map_or_else(
            || {
                let hotline = HotlineTransaction::request_from_params(107, 1, &params)
                    .map_err(|e| e.to_string())?;
                let wireframe_bytes =
                    encode_to_vec(&hotline, hotline_config()).map_err(|e| e.to_string())?;
                let legacy_tx = legacy_transaction_from_params(107, 1, &params)?;
                let legacy_bytes = self.rt.block_on(legacy_encode(&legacy_tx))?;
                Ok(EncodingResult {
                    wireframe_bytes,
                    legacy_bytes,
                })
            },
            |tx| {
                let legacy_tx = tx.clone();
                let hotline = HotlineTransaction::try_from(tx).map_err(|e| e.to_string())?;
                let wireframe_bytes =
                    encode_to_vec(&hotline, hotline_config()).map_err(|e| e.to_string())?;
                let legacy_bytes = self.rt.block_on(legacy_encode(&legacy_tx))?;
                Ok(EncodingResult {
                    wireframe_bytes,
                    legacy_bytes,
                })
            },
        );
        self.outcome.borrow_mut().replace(result);
    }

    fn with_outcome<T>(&self, f: impl FnOnce(&Result<EncodingResult, String>) -> T) -> T {
        let outcome_ref = self.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("encoding not executed");
        };
        f(outcome)
    }
}

fn legacy_transaction_from_params(
    ty: u16,
    id: u32,
    params: &[(FieldId, Vec<u8>)],
) -> Result<Transaction, String> {
    let payload = if params.is_empty() {
        Vec::new()
    } else {
        let param_slices: Vec<(FieldId, &[u8])> = params
            .iter()
            .map(|(field_id, bytes)| (*field_id, bytes.as_slice()))
            .collect();
        encode_params(&param_slices).map_err(|e| e.to_string())?
    };
    let payload_len = payload.len();
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty,
        id,
        error: 0,
        total_size: u32::try_from(payload_len).expect("payload length fits u32"),
        data_size: u32::try_from(payload_len).expect("payload length fits u32"),
    };
    Ok(Transaction { header, payload })
}

fn oversized_params() -> Vec<(FieldId, Vec<u8>)> {
    let per_param_len = u16::MAX as usize;
    let per_param_total = 4usize + per_param_len;
    let header_overhead = 2usize;
    let target = MAX_PAYLOAD_SIZE + 1;
    let params_needed = (target - header_overhead).div_ceil(per_param_total);
    (0..params_needed)
        .map(|idx| {
            let field_id = FieldId::Other(u16::try_from(9000 + idx).expect("field id fits u16"));
            (field_id, vec![0u8; per_param_len])
        })
        .collect()
}

async fn legacy_encode(tx: &Transaction) -> Result<Vec<u8>, String> {
    let (client, server) = tokio::io::duplex(64 * 1024);
    let transaction = tx.clone();
    let writer_task = tokio::spawn(async move {
        let mut writer = TransactionWriter::new(server);
        writer
            .write_transaction(&transaction)
            .await
            .map_err(|e| e.to_string())
    });
    let mut reader = client;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    writer_task
        .await
        .map_err(|e| e.to_string())?
        .map(|()| bytes)
}

fn count_frames(bytes: &[u8]) -> Result<usize, String> {
    let mut offset = 0usize;
    let mut count = 0usize;
    while offset < bytes.len() {
        let hdr = bytes
            .get(offset..offset + mxd::transaction::HEADER_LEN)
            .ok_or("truncated header")?;
        let hdr_arr: [u8; mxd::transaction::HEADER_LEN] = hdr
            .try_into()
            .map_err(|_| "header slice conversion failed")?;
        let header = FrameHeader::from_bytes(&hdr_arr);
        offset += mxd::transaction::HEADER_LEN;
        let data_size = usize::try_from(header.data_size).map_err(|_| "data size overflow")?;
        offset = offset.checked_add(data_size).ok_or("frame size overflow")?;
        count += 1;
    }
    if offset != bytes.len() {
        return Err("encoded bytes contain trailing data".to_owned());
    }
    Ok(count)
}

#[expect(
    clippy::allow_attributes,
    reason = "rstest-bdd macro expansion produces braces"
)]
#[allow(
    unused_braces,
    reason = "rustfmt requires braces for rstest-bdd fixtures"
)]
#[fixture]
fn world() -> EncodingWorld {
    #[allow(unused_braces, reason = "rustfmt requires braces")]
    {
        EncodingWorld::new()
    }
}

#[given("a parameter transaction with {count} field")]
fn given_parameter_transaction(world: &EncodingWorld, count: usize) {
    let params = match count {
        0 => Vec::new(),
        1 => vec![(FieldId::Login, b"alice".to_vec())],
        _ => panic!("unsupported parameter count for scenario"),
    };
    world.set_params(params);
}

#[given("a parameter transaction with a 40000-byte field value")]
fn given_large_parameter_transaction(world: &EncodingWorld) {
    world.set_params(vec![(FieldId::Other(999), vec![0u8; 40_000])]);
}

#[given("a transaction with mismatched header and payload sizes")]
fn given_mismatched_transaction(world: &EncodingWorld) {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 2,
        data_size: 2,
    };
    world.set_transaction(Transaction {
        header,
        payload: Vec::new(),
    });
}

#[given("a valid transaction with {count} field")]
fn given_valid_transaction(world: &EncodingWorld, count: usize) {
    let params = match count {
        0 => Vec::new(),
        1 => vec![(FieldId::Login, b"alice".to_vec())],
        _ => panic!("unsupported parameter count for scenario"),
    };
    let tx = legacy_transaction_from_params(107, 1, &params).expect("valid transaction");
    world.set_transaction(tx);
}

#[given("a transaction with invalid flags")]
fn given_transaction_with_invalid_flags(world: &EncodingWorld) {
    let header = FrameHeader {
        flags: 1,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    world.set_transaction(Transaction {
        header,
        payload: Vec::new(),
    });
}

#[given("a transaction with an oversized payload")]
fn given_transaction_with_oversized_payload(world: &EncodingWorld) {
    let payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    world.set_transaction(Transaction { header, payload });
}

#[given("a transaction with an invalid parameter payload")]
fn given_transaction_with_invalid_payload_structure(world: &EncodingWorld) {
    let payload = vec![0u8; 1];
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: 1,
        data_size: 1,
    };
    world.set_transaction(Transaction { header, payload });
}

#[given("a parameter transaction that exceeds the maximum payload size")]
fn given_oversized_parameter_transaction(world: &EncodingWorld) {
    world.set_params(oversized_params());
}

#[when("I encode the transaction")]
fn when_encode(world: &EncodingWorld) { world.encode(); }

#[then("encoding succeeds")]
fn then_succeeds(world: &EncodingWorld) {
    world.with_outcome(|outcome| {
        assert_step_ok!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
    });
}

#[then("the encoded bytes match the legacy encoder")]
fn then_bytes_match(world: &EncodingWorld) {
    world.with_outcome(|outcome| {
        let result = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
        assert_eq!(result.wireframe_bytes, result.legacy_bytes);
    });
}

#[then("the encoded transaction is fragmented into {frames} frames")]
fn then_fragmented(world: &EncodingWorld, frames: usize) {
    world.with_outcome(|outcome| {
        let result = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
        let count = count_frames(&result.wireframe_bytes).expect("frame count");
        assert_eq!(count, frames);
    });
}

#[then("encoding fails with \"{message}\"")]
fn then_fails(world: &EncodingWorld, message: String) {
    world.with_outcome(|outcome| {
        let text = assert_step_err!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
        assert!(
            text.contains(&message),
            "expected '{text}' to contain '{message}'"
        );
    });
}

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 0
)]
fn single_frame_param_transaction(world: EncodingWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 1
)]
fn empty_param_transaction(world: EncodingWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 2
)]
fn fragmented_param_transaction(world: EncodingWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 3
)]
fn try_from_transaction_succeeds(world: EncodingWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 4
)]
fn rejects_invalid_flags(world: EncodingWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 5
)]
fn rejects_oversized_payload(world: EncodingWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 6
)]
fn rejects_invalid_payload_structure(world: EncodingWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 7
)]
fn rejects_oversized_params(world: EncodingWorld) { let _ = world; }

#[scenario(
    path = "tests/features/wireframe_transaction_encoding.feature",
    index = 8
)]
fn rejects_size_mismatch(world: EncodingWorld) { let _ = world; }
