//! Case handling for the AFL transaction-parser target.
//!
//! The binary in `main.rs` only hands AFL's test cases to [`handle_case`],
//! which applies the policy and calls [`run_case`]. The logic lives here so
//! that it can be tested without the AFL runtime, which only `cargo afl` links.

use mxd::transaction::{HEADER_LEN, MAX_PAYLOAD_SIZE, TransactionError, parse_transaction};

/// The largest input, in bytes, that the harness parses: a header and a
/// maximum-size payload.
pub const MAX_INPUT_LEN: usize = HEADER_LEN + MAX_PAYLOAD_SIZE;

/// Report whether an input is too large for the harness to parse.
///
/// An input of exactly [`MAX_INPUT_LEN`] bytes is parsed.
///
/// # Examples
///
/// ```
/// assert!(!fuzz::is_oversized(fuzz::MAX_INPUT_LEN));
/// assert!(fuzz::is_oversized(fuzz::MAX_INPUT_LEN + 1));
/// ```
#[must_use]
pub const fn is_oversized(len: usize) -> bool { len > MAX_INPUT_LEN }

/// What the harness did with one AFL test case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The input was over [`MAX_INPUT_LEN`] and was not parsed.
    Skipped,
    /// The parser accepted the input.
    Accepted,
    /// The parser rejected the input with an error, as it should a malformed
    /// frame. This is not a crash.
    Rejected,
}

/// Run one input through the transaction parser.
///
/// # Errors
///
/// Returns the parser's error when it rejects the input. [`handle_case`] treats
/// that as [`Outcome::Rejected`], an ordinary result, not a panic.
pub fn run_case(data: &[u8]) -> Result<(), TransactionError> { parse_transaction(data).map(drop) }

/// Handle one AFL test case, counting only a panic or abort as a crash.
///
/// An oversized input is skipped, not truncated, so a mutated case cannot make
/// the parser allocate without bound and a prefix of it is never mistaken for
/// the case AFL saved. A parser that rejects a malformed frame is working, so
/// its error is a normal outcome. Treating it as a crash would make AFL save
/// nearly every mutation and bury a real defect: a panic, an overflow or an
/// abort inside the parser, which still propagates from here.
pub fn handle_case(data: &[u8]) -> Outcome { handle_with(data, run_case) }

/// Apply the policy of [`handle_case`] to any parser: skip an oversized input,
/// map `Ok` and `Err` to an outcome, and let a panic in `parse` propagate.
fn handle_with<E>(data: &[u8], parse: impl FnOnce(&[u8]) -> Result<(), E>) -> Outcome {
    if is_oversized(data.len()) {
        return Outcome::Skipped;
    }
    match parse(data) {
        Ok(()) => Outcome::Accepted,
        Err(_) => Outcome::Rejected,
    }
}

#[cfg(test)]
mod tests {
    //! Tests of the fuzz-case policy, run without the AFL runtime.

    use std::cell::Cell;

    use super::*;

    /// A well-formed, empty-payload transaction frame.
    fn valid_frame() -> Vec<u8> {
        use mxd::transaction::{FrameHeader, Transaction};

        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 0,
            id: 1,
            error: 0,
            total_size: 0,
            data_size: 0,
        };
        Transaction {
            header,
            payload: Vec::new(),
        }
        .to_bytes()
    }

    #[test]
    fn the_limit_is_a_header_and_a_maximum_payload() {
        assert_eq!(MAX_INPUT_LEN, HEADER_LEN + MAX_PAYLOAD_SIZE);
        assert!(!is_oversized(MAX_INPUT_LEN));
        assert!(is_oversized(MAX_INPUT_LEN + 1));
    }

    #[test]
    fn a_valid_frame_is_accepted() {
        assert!(run_case(&valid_frame()).is_ok());
        assert_eq!(handle_case(&valid_frame()), Outcome::Accepted);
    }

    #[test]
    fn a_malformed_input_is_rejected_not_a_crash() {
        assert!(run_case(&[0xff; 3]).is_err());
        assert_eq!(handle_case(&[0xff; 3]), Outcome::Rejected);
    }

    #[test]
    fn an_oversized_input_is_skipped_not_truncated() {
        // Junk the parser would reject if any of it were parsed, so a skip is
        // distinguishable from a truncate-and-parse.
        assert_eq!(
            handle_case(&vec![0xff; MAX_INPUT_LEN + 1]),
            Outcome::Skipped
        );
    }

    #[test]
    fn an_input_of_exactly_the_limit_is_parsed() {
        assert_eq!(handle_case(&vec![0xff; MAX_INPUT_LEN]), Outcome::Rejected);
    }

    #[test]
    fn the_handler_gives_the_parser_the_case_once_and_skips_it_when_oversized() {
        let calls = Cell::new(0);
        let parse = |data: &[u8]| {
            calls.set(calls.get() + 1);
            assert_eq!(data, [1, 2]);
            Ok::<(), ()>(())
        };
        assert_eq!(handle_with(&[1, 2], parse), Outcome::Accepted);
        assert_eq!(calls.get(), 1);

        let skipped = Cell::new(false);
        let outcome = handle_with(&vec![0; MAX_INPUT_LEN + 1], |_| {
            skipped.set(true);
            Ok::<(), ()>(())
        });
        assert_eq!(outcome, Outcome::Skipped);
        assert!(!skipped.get(), "an oversized input reached the parser");
    }

    #[test]
    fn a_parser_error_is_an_outcome_not_a_crash() {
        assert_eq!(
            handle_with(&[], |_| Err::<(), &str>("rejected")),
            Outcome::Rejected
        );
    }

    #[test]
    #[should_panic(expected = "parser defect")]
    fn a_panic_in_the_parser_is_still_a_crash() {
        handle_with::<()>(&[], |_| panic!("parser defect"));
    }
}
