#![expect(clippy::expect_used, reason = "test assertions")]

//! Behavioural tests for streaming transaction framing.

use std::{cell::RefCell, io::Cursor};

use mxd::{
    transaction::{FrameHeader, TransactionFragment, TransactionStreamReader},
    wireframe::test_helpers::{
        fragmented_transaction_bytes,
        mismatched_continuation_bytes,
        transaction_bytes,
    },
};
use rstest::fixture;
use rstest_bdd::{assert_step_err, assert_step_ok};
use rstest_bdd_macros::{given, scenario, then, when};
use tokio::{io::BufReader, runtime::Runtime};

struct StreamingWorld {
    bytes: RefCell<Vec<u8>>,
    outcome: RefCell<Option<Result<Vec<TransactionFragment>, String>>>,
    rt: Runtime,
}

impl StreamingWorld {
    fn new() -> Self {
        let rt = Runtime::new().expect("runtime");
        Self {
            bytes: RefCell::new(Vec::new()),
            outcome: RefCell::new(None),
            rt,
        }
    }

    fn set_bytes(&self, bytes: Vec<u8>) { *self.bytes.borrow_mut() = bytes; }

    fn stream_fragments(&self, limit: usize) {
        let bytes = self.bytes.borrow().clone();
        let result = self.rt.block_on(async move {
            let mut reader = TransactionStreamReader::new(BufReader::new(Cursor::new(bytes)))
                .with_max_total(limit);
            let mut stream = reader.start_transaction().await?;
            let mut fragments = Vec::new();
            while let Some(fragment) = stream.next_fragment().await? {
                fragments.push(fragment);
            }
            Ok::<_, mxd::transaction::TransactionError>(fragments)
        });
        self.outcome
            .borrow_mut()
            .replace(result.map_err(|e| e.to_string()));
    }

    fn with_fragments<T>(&self, f: impl FnOnce(&[TransactionFragment]) -> T) -> T {
        let outcome_ref = self.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("streaming not executed");
        };
        let fragments = assert_step_ok!(outcome.as_ref().map_err(ToString::to_string));
        f(fragments)
    }

    fn assert_failure_contains(&self, expected: &str) {
        let outcome_ref = self.outcome.borrow();
        let Some(outcome) = outcome_ref.as_ref() else {
            panic!("streaming not executed");
        };
        let text = assert_step_err!(outcome.as_ref().map_err(ToString::to_string));
        assert!(text.contains(expected), "expected '{expected}' in '{text}'");
    }
}

#[fixture]
fn world() -> StreamingWorld {
    #[expect(
        clippy::allow_attributes,
        reason = "cannot use expect due to macro interaction"
    )]
    #[allow(unused_braces, reason = "rustfmt requires braces")]
    {
        StreamingWorld::new()
    }
}

fn build_payload(size: usize) -> Vec<u8> { vec![0u8; size] }

#[given("a transaction with total size {total} and data size {data}")]
fn given_transaction_sizes(world: &StreamingWorld, total: u32, data: u32) {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 107,
        id: 1,
        error: 0,
        total_size: total,
        data_size: data,
    };
    let payload = build_payload(data as usize);
    world.set_bytes(transaction_bytes(&header, &payload));
}

#[given("a fragmented transaction with total size {total} across {count} fragments")]
fn given_fragmented_transaction(world: &StreamingWorld, total: usize, count: usize) {
    assert!(count > 0, "fragment count must be positive");
    let payload = build_payload(total);
    let fragment_size = total.div_ceil(count).max(1);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 410,
        id: 7,
        error: 0,
        total_size: u32::try_from(total).expect("total fits u32"),
        data_size: u32::try_from(total).expect("total fits u32"),
    };
    let fragments =
        fragmented_transaction_bytes(&header, &payload, fragment_size).expect("fragments");
    let bytes: Vec<u8> = fragments.into_iter().flatten().collect();
    world.set_bytes(bytes);
}

#[given("a fragmented transaction with mismatched continuation headers")]
fn given_mismatched_continuation(world: &StreamingWorld) {
    let bytes = mismatched_continuation_bytes().expect("mismatched bytes");
    world.set_bytes(bytes);
}

#[when("I stream the transaction fragments with a limit of {limit} bytes")]
fn when_stream(world: &StreamingWorld, limit: usize) { world.stream_fragments(limit); }

#[then("streaming succeeds")]
fn then_streaming_succeeds(world: &StreamingWorld) {
    let outcome_ref = world.outcome.borrow();
    let Some(outcome) = outcome_ref.as_ref() else {
        panic!("streaming not executed");
    };
    assert_step_ok!(outcome.as_ref().map(|_| ()).map_err(ToString::to_string));
}

#[then("I receive {count} fragments")]
fn then_fragment_count(world: &StreamingWorld, count: usize) {
    world.with_fragments(|fragments| {
        assert_eq!(fragments.len(), count);
    });
}

#[then("the total streamed size is {total} bytes")]
fn then_total_size(world: &StreamingWorld, total: usize) {
    world.with_fragments(|fragments| {
        let sum: usize = fragments.iter().map(|f| f.payload.len()).sum();
        assert_eq!(sum, total);
    });
}

#[then("each fragment is at most {max} bytes")]
fn then_fragment_max(world: &StreamingWorld, max: usize) {
    world.with_fragments(|fragments| {
        for fragment in fragments {
            assert!(fragment.payload.len() <= max);
        }
    });
}

#[then("streaming fails with error \"{message}\"")]
fn then_streaming_fails(world: &StreamingWorld, message: String) {
    world.assert_failure_contains(&message);
}

#[scenario(path = "tests/features/transaction_streaming.feature", index = 0)]
fn streaming_multi_fragment(world: StreamingWorld) { let _ = world; }

#[scenario(path = "tests/features/transaction_streaming.feature", index = 1)]
fn streaming_rejects_mismatch(world: StreamingWorld) { let _ = world; }

#[scenario(path = "tests/features/transaction_streaming.feature", index = 2)]
fn streaming_rejects_limit(world: StreamingWorld) { let _ = world; }
