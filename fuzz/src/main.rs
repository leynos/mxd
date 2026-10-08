//! AFL fuzz target for transaction parsing.
//!
//! Feeds each AFL test case to the transaction parser. `afl::fuzz!` supplies
//! the persistent-mode loop and links the AFL runtime that provides
//! `__AFL_LOOP`; declaring that symbol by hand left it undefined at link time.

use mxd::transaction::{HEADER_LEN, MAX_PAYLOAD_SIZE, parse_transaction};

fn main() {
    afl::fuzz!(|data: &[u8]| {
        // Ignore anything larger than the maximum frame so a mutated test case
        // cannot make the parser allocate without bound.
        if data.len() > HEADER_LEN + MAX_PAYLOAD_SIZE {
            return;
        }

        // Panic on parse errors so AFL can detect crashes.
        #[expect(clippy::unwrap_used, reason = "AFL fuzz target: crash on parse errors")]
        parse_transaction(data).unwrap();
    });
}
