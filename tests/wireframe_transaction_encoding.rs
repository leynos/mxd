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
use rstest_bdd_macros::{given, scenarios, then, when};
use tokio::io::AsyncReadExt;

fn hotline_config() -> impl bincode::config::Config {
    config::standard()
        .with_big_endian()
        .with_fixed_int_encoding()
}

struct EncodingWorld {
    params: RefCell<Vec<(FieldId, Vec<u8>)>>,
    transaction: RefCell<Option<Transaction>>,
    setup_error: RefCell<Option<String>>,
    outcome: RefCell<Option<Result<EncodingResult, String>>>,
}

struct EncodingResult {
    wireframe_bytes: Vec<u8>,
    legacy_bytes: Vec<u8>,
}

impl EncodingWorld {
    const fn new() -> Self {
        Self {
            params: RefCell::new(Vec::new()),
            transaction: RefCell::new(None),
            setup_error: RefCell::new(None),
            outcome: RefCell::new(None),
        }
    }

    fn set_params(&self, params: Vec<(FieldId, Vec<u8>)>) {
        self.setup_error.borrow_mut().take();
        *self.transaction.borrow_mut() = None;
        *self.params.borrow_mut() = params;
    }

    fn set_transaction(&self, tx: Transaction) {
        self.setup_error.borrow_mut().take();
        self.params.borrow_mut().clear();
        *self.transaction.borrow_mut() = Some(tx);
    }

    fn set_params_result(&self, params: Result<Vec<(FieldId, Vec<u8>)>, String>) {
        match params {
            Ok(parsed_params) => self.set_params(parsed_params),
            Err(err) => {
                *self.transaction.borrow_mut() = None;
                self.params.borrow_mut().clear();
                self.setup_error.borrow_mut().replace(err);
            }
        }
    }

    fn set_transaction_result(&self, tx: Result<Transaction, String>) {
        match tx {
            Ok(transaction) => self.set_transaction(transaction),
            Err(err) => {
                *self.transaction.borrow_mut() = None;
                self.params.borrow_mut().clear();
                self.setup_error.borrow_mut().replace(err);
            }
        }
    }

    async fn encode(&self) {
        if let Some(err) = self.setup_error.borrow().clone() {
            self.outcome.borrow_mut().replace(Err(err));
            return;
        }
        let params = self.params.borrow().clone();
        let maybe_tx = self.transaction.borrow().clone();
        let result: Result<EncodingResult, String> = async {
            if let Some(tx) = maybe_tx {
                let legacy_tx = tx.clone();
                let hotline = HotlineTransaction::try_from(tx).map_err(|e| e.to_string())?;
                let wireframe_bytes =
                    encode_to_vec(&hotline, hotline_config()).map_err(|e| e.to_string())?;
                let legacy_bytes = legacy_encode(&legacy_tx).await?;
                Ok(EncodingResult {
                    wireframe_bytes,
                    legacy_bytes,
                })
            } else {
                let hotline = HotlineTransaction::request_from_params(107, 1, &params)
                    .map_err(|e| e.to_string())?;
                let wireframe_bytes =
                    encode_to_vec(&hotline, hotline_config()).map_err(|e| e.to_string())?;
                let legacy_tx = legacy_transaction_from_params(107, 1, &params)?;
                let legacy_bytes = legacy_encode(&legacy_tx).await?;
                Ok(EncodingResult {
                    wireframe_bytes,
                    legacy_bytes,
                })
            }
        }
        .await;
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
    let payload_len_u32 =
        u32::try_from(payload_len).map_err(|_| "payload length overflows u32".to_owned())?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty,
        id,
        error: 0,
        total_size: payload_len_u32,
        data_size: payload_len_u32,
    };
    Ok(Transaction { header, payload })
}

fn oversized_params() -> Result<Vec<(FieldId, Vec<u8>)>, String> {
    let per_param_len = u16::MAX as usize;
    let per_param_total = 4usize + per_param_len;
    let header_overhead = 2usize;
    let target = MAX_PAYLOAD_SIZE + 1;
    let params_needed = (target - header_overhead).div_ceil(per_param_total);
    (0..params_needed)
        .map(|idx| {
            let raw = u16::try_from(9000 + idx).map_err(|_| "field id overflows u16".to_owned())?;
            Ok((FieldId::Other(raw), vec![0u8; per_param_len]))
        })
        .collect::<Result<Vec<_>, String>>()
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

#[fixture]
fn world() -> EncodingWorld {
    // Keep fixture setup non-const so each scenario instantiates fresh state.
    std::hint::black_box(());
    EncodingWorld::new()
}

/// Test helper struct for setting up transaction headers.
#[derive(Clone, Copy)]
struct TestHeaderParams {
    flags: u8,
    is_reply: u8,
    total_size: u32,
    data_size: u32,
}

impl TestHeaderParams {
    /// Creates header params with custom values.
    const fn new(flags: u8, is_reply: u8, total_size: u32, data_size: u32) -> Self {
        Self {
            flags,
            is_reply,
            total_size,
            data_size,
        }
    }

    /// Creates header params for a typical test case with zeros.
    const fn zeros() -> Self {
        Self {
            flags: 0,
            is_reply: 0,
            total_size: 0,
            data_size: 0,
        }
    }
}

fn set_test_transaction(world: &EncodingWorld, header_params: TestHeaderParams, payload: Vec<u8>) {
    let header = FrameHeader {
        flags: header_params.flags,
        is_reply: header_params.is_reply,
        ty: 107,
        id: 1,
        error: 0,
        total_size: header_params.total_size,
        data_size: header_params.data_size,
    };

    world.set_transaction(Transaction { header, payload });
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

#[given("a parameter transaction with a {size}-byte field value")]
fn given_large_parameter_transaction(world: &EncodingWorld, size: usize) {
    world.set_params(vec![(FieldId::Other(999), vec![0u8; size])]);
}

#[given("a transaction with mismatched header and payload sizes")]
fn given_mismatched_transaction(world: &EncodingWorld) {
    set_test_transaction(world, TestHeaderParams::new(0, 0, 2, 2), Vec::new());
}

#[given("a valid transaction with {count} field")]
fn given_valid_transaction(world: &EncodingWorld, count: usize) {
    let params = match count {
        0 => Vec::new(),
        1 => vec![(FieldId::Login, b"alice".to_vec())],
        _ => panic!("unsupported parameter count for scenario"),
    };
    world.set_transaction_result(legacy_transaction_from_params(107, 1, &params));
}

#[given("a transaction with invalid flags")]
fn given_transaction_with_invalid_flags(world: &EncodingWorld) {
    set_test_transaction(world, TestHeaderParams::new(1, 0, 0, 0), Vec::new());
}

#[given("a transaction with an oversized payload")]
fn given_transaction_with_oversized_payload(world: &EncodingWorld) {
    set_test_transaction(
        world,
        TestHeaderParams::zeros(),
        vec![0u8; MAX_PAYLOAD_SIZE + 1],
    );
}

#[given("a transaction with an invalid parameter payload")]
fn given_transaction_with_invalid_payload_structure(world: &EncodingWorld) {
    set_test_transaction(world, TestHeaderParams::new(0, 0, 1, 1), vec![0u8; 1]);
}

#[given("a parameter transaction that exceeds the maximum payload size")]
fn given_oversized_parameter_transaction(world: &EncodingWorld) {
    world.set_params_result(oversized_params());
}

#[when("I encode the transaction")]
async fn when_encode(world: &EncodingWorld) { world.encode().await; }

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
        let count = count_frames(&result.wireframe_bytes)
            .unwrap_or_else(|err| panic!("frame count should parse: {err}"));
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

scenarios!(
    "tests/features/wireframe_transaction_encoding.feature",
    runtime = "tokio-current-thread",
    fixtures = [world: EncodingWorld]
);
