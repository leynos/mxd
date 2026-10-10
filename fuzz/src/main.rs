//! AFL fuzz target for transaction parsing.
//!
//! Feeds each AFL test case to [`fuzz::handle_case`]. A rejected frame is not a
//! crash and a panic aborts the process, so the outcome needs no action here,
//! except that setting `MXD_FUZZ_TRACE` prints it, one word per case, to
//! standard error. `scripts/replay_harness.sh` uses that to prove the built
//! harness hands its input to the parser. `afl::fuzz!` supplies the
//! persistent-mode loop and links the AFL runtime that provides `__AFL_LOOP`;
//! declaring that symbol by hand left it undefined at link time.

/// Hand each AFL test case to the transaction parser, in AFL's persistent mode.
fn main() {
    let trace = std::env::var_os("MXD_FUZZ_TRACE").is_some();
    afl::fuzz!(|data: &[u8]| {
        let outcome = fuzz::handle_case(data);
        if trace {
            #[expect(clippy::print_stderr, reason = "the trace is the harness's output")]
            {
                eprintln!("{outcome:?}");
            }
        }
    });
}
