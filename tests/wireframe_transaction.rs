//! Behavioural and property tests for the wireframe transaction codec.

use std::cell::RefCell;

use bincode::{borrow_decode_from_slice, config, error::DecodeError};
use mxd::{
    transaction::{FrameHeader, MAX_FRAME_DATA, MAX_PAYLOAD_SIZE},
    wireframe::{
        codec::HotlineTransaction,
        test_helpers::{
            fragmented_transaction_bytes,
            mismatched_continuation_bytes,
            transaction_bytes,
        },
    },
};
use proptest::prelude::*;
use rstest::fixture;
use rstest_bdd::{assert_step_err, assert_step_ok};
use rstest_bdd_macros::{given, scenarios, then, when};

/// Return the bincode configuration for Hotline transaction decoding.
///
/// Uses big-endian byte order and fixed-width integer encoding as required by
/// the Hotline protocol.
fn hotline_config() -> impl bincode::config::Config {
    config::standard()
        .with_big_endian()
        .with_fixed_int_encoding()
}

// -----------------------------------------------------------------------------
// BDD World and Step Definitions
// -----------------------------------------------------------------------------

#[derive(Default)]
struct TransactionWorld {
    bytes: RefCell<Vec<u8>>,
    outcome: RefCell<Option<Result<HotlineTransaction, DecodeError>>>,
}

impl TransactionWorld {
    fn set_bytes(&self, bytes: &[u8]) {
        let mut target = self.bytes.borrow_mut();
        target.clear();
        target.extend_from_slice(bytes);
    }

    fn decode(&self) {
        let result = borrow_decode_from_slice::<HotlineTransaction, _>(
            &self.bytes.borrow(),
            hotline_config(),
        )
        .map(|(tx, _)| tx);
        self.outcome.borrow_mut().replace(result);
    }

    /// Access the decode outcome, panicking if decode was not executed.
    fn with_outcome<T>(&self, f: impl FnOnce(&Result<HotlineTransaction, DecodeError>) -> T) -> T {
        let outcome_ref = self.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("decode not executed");
        };
        f(outcome)
    }
}

#[expect(
    clippy::allow_attributes,
    reason = "rustc compiler does not emit expected lint"
)]
#[allow(unused_braces, reason = "rstest-bdd macro expansion produces braces")]
#[fixture]
fn world() -> TransactionWorld { TransactionWorld::default() }

fn build_valid_payload(size: usize) -> Vec<u8> {
    if size == 0 {
        return Vec::new();
    }
    let mut payload = vec![0u8; size];
    // Set param-count to 0 (requires at least 2 bytes)
    if let Some(first) = payload.get_mut(0) {
        *first = 0;
    }
    if let Some(second) = payload.get_mut(1) {
        *second = 0;
    }
    payload
}

/// Build a fixture transaction for BDD steps using shared default header values.
fn build_transaction(world: &TransactionWorld, flags: u8, total_size: u32, data_size: u32) {
    let header = FrameHeader {
        flags,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size,
        data_size,
    };
    let payload = build_valid_payload(data_size as usize);
    world.set_bytes(&transaction_bytes(&header, &payload));
}

#[given("a transaction with total size {total} and data size {data}")]
fn given_transaction_sizes(world: &TransactionWorld, total: u32, data: u32) {
    build_transaction(world, 0, total, data);
}

#[given("a transaction with flags {flags}")]
fn given_transaction_flags(world: &TransactionWorld, flags: u8) {
    build_transaction(world, flags, 0, 0);
}

#[given("a fragmented transaction with total size {total} across {count} fragments")]
fn given_fragmented_transaction(world: &TransactionWorld, total: usize, count: usize) {
    assert!(
        count > 0,
        "fragment count must be positive in BDD scenarios"
    );
    let payload = build_valid_payload(total);
    let fragment_size = total.div_ceil(count).max(1);
    let Ok(total_u32) = u32::try_from(total) else {
        panic!("total size must fit in u32 for test");
    };
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: total_u32,
        data_size: total_u32,
    };
    let fragments = match fragmented_transaction_bytes(&header, &payload, fragment_size) {
        Ok(frags) => frags,
        Err(err) => panic!("chunk size fits in u32 for test: {err:?}"),
    };
    let bytes: Vec<u8> = fragments.into_iter().flatten().collect();
    world.set_bytes(&bytes);
}

#[given("a fragmented transaction with mismatched continuation headers")]
fn given_mismatched_continuation(world: &TransactionWorld) {
    let bytes = mismatched_continuation_bytes()
        .unwrap_or_else(|err| panic!("test values fit in u32: {err:?}"));
    world.set_bytes(&bytes);
}

#[when("I decode the transaction frame")]
fn when_decode(world: &TransactionWorld) { world.decode(); }

#[then("decoding succeeds")]
fn then_success(world: &TransactionWorld) {
    world.with_outcome(|outcome| {
        assert_step_ok!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
    });
}

#[then("the payload length is {len}")]
fn then_payload_length(world: &TransactionWorld, len: usize) {
    world.with_outcome(|outcome| {
        let tx = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
        assert_eq!(tx.payload().len(), len);
    });
}

#[then("decoding fails with \"{message}\"")]
fn then_failure(world: &TransactionWorld, message: String) {
    world.with_outcome(|outcome| {
        let text = assert_step_err!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
        assert!(
            text.contains(&message),
            "expected '{text}' to contain '{message}'"
        );
    });
}

scenarios!(
    "tests/features/wireframe_transaction.feature",
    fixtures = [world: TransactionWorld]
);

// -----------------------------------------------------------------------------
// Property Tests
// -----------------------------------------------------------------------------

/// Check if a length combination is invalid per protocol spec.
const fn is_invalid_combination(data_size: u32, total_size: u32) -> bool {
    data_size > total_size
        || (data_size == 0 && total_size > 0)
        || total_size as usize > MAX_PAYLOAD_SIZE
        || data_size as usize > MAX_FRAME_DATA
}

proptest! {
    /// Valid single-frame transactions decode successfully.
    #[test]
    fn roundtrip_valid_single_frame(
        ty in 100u16..500u16,
        id in 1u32..u32::MAX,
        payload_len in 0usize..1000usize,
    ) {
        // Skip payload_len == 1 (invalid: can't set param-count with only 1 byte)
        if payload_len == 1 {
            return Ok(());
        }
        let payload = build_valid_payload(payload_len);
        let Ok(total) = u32::try_from(payload.len()) else {
            return Err(TestCaseError::fail("payload length exceeds u32"));
        };
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty,
            id,
            error: 0,
            total_size: total,
            data_size: total,
        };
        let bytes = transaction_bytes(&header, &payload);

        let result = borrow_decode_from_slice::<HotlineTransaction, _>(&bytes, hotline_config());

        prop_assert!(result.is_ok(), "decode failed: {:?}", result.err());
        let (tx, _) = match result {
            Ok(val) => val,
            Err(err) => return Err(TestCaseError::fail(format!("decode failed: {err}"))),
        };
        prop_assert_eq!(tx.header().ty, ty);
        prop_assert_eq!(tx.header().id, id);
        prop_assert_eq!(tx.payload().len(), payload.len());
    }

    /// Multi-fragment transactions reassemble correctly.
    #[test]
    fn multi_fragment_reassembly(
        ty in 100u16..500u16,
        id in 1u32..u32::MAX,
        total_size in 4usize..65536usize,
        fragment_size in 100usize..MAX_FRAME_DATA,
    ) {
        // Bias towards true multi-fragment cases by discarding inputs where the
        // fragment size would collapse everything into a single frame or a
        // single full fragment.
        //
        // Since `total_size >= 4`, requiring `fragment_size * 2 <= total_size`
        // guarantees at least two fragments and increases the chance of
        // exercising a final partial fragment in the reassembly logic.
        prop_assume!(fragment_size.saturating_mul(2) <= total_size);

        let payload = build_valid_payload(total_size);
        let Ok(total_u32) = u32::try_from(total_size) else {
            return Err(TestCaseError::fail("test total fits in u32"));
        };
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty,
            id,
            error: 0,
            total_size: total_u32,
            data_size: total_u32,
        };

        let fragments = match fragmented_transaction_bytes(&header, &payload, fragment_size) {
            Ok(frags) => frags,
            Err(err) => return Err(TestCaseError::fail(format!("chunk size fits in u32: {err}"))),
        };
        let bytes: Vec<u8> = fragments.into_iter().flatten().collect();

        let result = borrow_decode_from_slice::<HotlineTransaction, _>(&bytes, hotline_config());

        prop_assert!(result.is_ok(), "decode failed: {:?}", result.err());
        let (tx, _) = match result {
            Ok(val) => val,
            Err(err) => return Err(TestCaseError::fail(format!("decode failed: {err}"))),
        };
        prop_assert_eq!(tx.header().total_size, total_u32);
        prop_assert_eq!(tx.payload().len(), total_size);
    }

    /// Invalid length combinations are always rejected.
    #[test]
    fn rejects_invalid_lengths(
        data_size in 0u32..100_000u32,
        total_size in 0u32..2_000_000u32,
    ) {
        prop_assume!(is_invalid_combination(data_size, total_size));

        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 1,
            error: 0,
            total_size,
            data_size,
        };
        let payload = vec![0u8; data_size as usize];
        let bytes = transaction_bytes(&header, &payload);

        let result = borrow_decode_from_slice::<HotlineTransaction, _>(&bytes, hotline_config());

        prop_assert!(result.is_err(), "expected rejection for data={data_size}, total={total_size}");
    }
}
