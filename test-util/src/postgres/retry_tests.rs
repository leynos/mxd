//! Regression and property tests for the `PostgreSQL` slot retry policy.

use std::{
    cell::{Cell, RefCell},
    future::ready,
    time::Duration,
};

use futures_util::FutureExt;
use proptest::prelude::*;
use rstest::rstest;

use super::{SLOT_CONTENTION_ATTEMPTS, retry_slot_contention};
use crate::postgres::test_errors::NestedError;

#[rstest]
#[case(2, true, 3, vec![25, 50])]
#[case(SLOT_CONTENTION_ATTEMPTS, false, SLOT_CONTENTION_ATTEMPTS, vec![25, 50, 75, 100, 125, 150, 175])]
fn retries_nested_slot_errors_with_bounded_backoff(
    #[case] failures: u32,
    #[case] succeeds: bool,
    #[case] expected_attempts: u32,
    #[case] expected_delays_ms: Vec<u64>,
) {
    let attempts = Cell::new(0);
    let delays = RefCell::new(Vec::new());
    let result = retry_slot_contention(
        || {
            attempts.set(attempts.get() + 1);
            ready(if attempts.get() <= failures {
                Err(NestedError::new("slot lock /test is already held"))
            } else {
                Ok(())
            })
        },
        |delay| {
            delays.borrow_mut().push(delay);
            ready(())
        },
    )
    .now_or_never()
    .expect("ready callbacks never suspend");
    assert_eq!(result.is_ok(), succeeds);
    assert_eq!(attempts.get(), expected_attempts);
    assert_eq!(
        *delays.borrow(),
        expected_delays_ms
            .into_iter()
            .map(Duration::from_millis)
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn asynchronous_retry_yields_and_recovers_from_nested_slot_error() {
    let attempts = Cell::new(0);
    let delays = RefCell::new(Vec::new());
    let result = retry_slot_contention(
        || {
            attempts.set(attempts.get() + 1);
            ready(if attempts.get() == 1 {
                Err(NestedError::new("slot lock /test is already held"))
            } else {
                Ok(())
            })
        },
        |delay| {
            delays.borrow_mut().push(delay);
            tokio::task::yield_now()
        },
    )
    .await;
    assert!(result.is_ok());
    assert_eq!(attempts.get(), 2);
    assert_eq!(*delays.borrow(), vec![Duration::from_millis(25)]);
}

proptest! {
    #[test]
    fn retry_policy_stops_at_first_terminal_event(failures in 0_u32..16, unrelated_at in 0_u32..16) {
        let attempts = Cell::new(0_u32);
        let delays = RefCell::new(Vec::new());
        let result = retry_slot_contention(
            || {
                let index = attempts.get();
                attempts.set(index + 1);
                ready(if index == unrelated_at {
                    Err(NestedError::new("unrelated startup failure"))
                } else if index < failures {
                    Err(NestedError::new("slot lock /test is already held"))
                } else {
                    Ok(())
                })
            },
            |delay| { delays.borrow_mut().push(delay); ready(()) },
        ).now_or_never().expect("ready callbacks never suspend");
        let terminal_index = failures.min(unrelated_at).min(SLOT_CONTENTION_ATTEMPTS - 1);
        let expected_attempts = terminal_index + 1;
        let expected_success = failures < SLOT_CONTENTION_ATTEMPTS && failures < unrelated_at;
        let expected_delays = (1..expected_attempts).map(|n| Duration::from_millis(u64::from(n) * 25)).collect::<Vec<_>>();
        prop_assert_eq!(attempts.get(), expected_attempts);
        prop_assert_eq!(result.is_ok(), expected_success);
        prop_assert_eq!(&*delays.borrow(), &expected_delays);
    }
}
