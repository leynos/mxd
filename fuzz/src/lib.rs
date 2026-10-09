//! Case handling for the AFL transaction-parser target.
//!
//! The binary in `main.rs` only hands AFL's test cases to [`run_case`]. The
//! logic lives here so that it can be tested without the AFL runtime, which
//! only `cargo afl` links.

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

/// Run one AFL test case through the transaction parser.
///
/// An oversized input is skipped, not truncated, so a mutated case cannot make
/// the parser allocate without bound and a prefix of it is never mistaken for
/// the case AFL saved.
///
/// # Errors
///
/// Returns the parser's error when it rejects the input. The caller decides
/// what that means: the AFL binary panics, which is how AFL detects a crash.
pub fn run_case(data: &[u8]) -> Result<(), TransactionError> {
    if is_oversized(data.len()) {
        return Ok(());
    }
    parse_transaction(data).map(drop)
}

/// Handle one AFL test case, counting only a panic or abort as a crash.
///
/// A parser that rejects a malformed frame is working, so its error is a
/// normal outcome. Treating it as a crash would make AFL save nearly every
/// mutation and bury a real defect: a panic, an overflow or an abort inside
/// the parser, which still propagates from here.
pub fn handle_case(data: &[u8]) { handle_with(data, run_case); }

/// Run `parse` on a case and accept either of its outcomes; a panic in `parse`
/// is not caught.
fn handle_with<E>(data: &[u8], parse: impl FnOnce(&[u8]) -> Result<(), E>) {
    match parse(data) {
        Ok(()) | Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
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
    fn a_valid_frame_is_parsed() {
        assert!(run_case(&valid_frame()).is_ok());
    }

    #[test]
    fn an_oversized_input_is_skipped_not_truncated() {
        // Junk that the parser would reject if any of it were parsed, so a
        // skip is distinguishable from a truncate-and-parse.
        assert!(run_case(&vec![0xff; MAX_INPUT_LEN + 1]).is_ok());
    }

    #[test]
    fn a_rejected_frame_is_not_a_crash() {
        handle_case(&[0xff; 3]);
        handle_case(&valid_frame());
        handle_with(&[], |_| Err::<(), &str>("rejected"));
    }

    #[test]
    #[should_panic(expected = "parser defect")]
    fn a_panic_in_the_parser_is_still_a_crash() {
        handle_with::<()>(&[], |_| panic!("parser defect"));
    }

    #[test]
    fn a_malformed_input_is_reported_as_an_error() {
        assert!(run_case(&[0xff; 3]).is_err());
    }
}
