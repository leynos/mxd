//! AFL fuzz target for transaction parsing.
//!
//! Feeds each AFL test case to [`fuzz::run_case`]. `afl::fuzz!` supplies the
//! persistent-mode loop and links the AFL runtime that provides `__AFL_LOOP`;
//! declaring that symbol by hand left it undefined at link time.

/// Hand each AFL test case to the transaction parser, in AFL's persistent mode.
fn main() {
    afl::fuzz!(|data: &[u8]| {
        // Panic on parse errors so AFL can detect crashes.
        #[expect(clippy::expect_used, reason = "AFL fuzz target: crash on parse errors")]
        fuzz::run_case(data).expect("transaction parse error");
    });
}
