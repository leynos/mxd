extern "C" {
    fn __AFL_LOOP(cnt: u32) -> i32;
}
use std::io::{self, Read};

use mxd::transaction::{parse_transaction, HEADER_LEN, MAX_PAYLOAD_SIZE};

fn main() {
    // Allocate a buffer up to the maximum frame size so we don't grow
    // indefinitely in persistent mode.
    let mut data = Vec::with_capacity(HEADER_LEN + MAX_PAYLOAD_SIZE);
    loop {
        if unsafe { __AFL_LOOP(1000) } == 0 { break; }
        data.clear();
        // Limit the amount of data read from each testcase to avoid
        // unbounded allocations. `take` will stop after the specified limit.
        if io::stdin()
            .take((HEADER_LEN + MAX_PAYLOAD_SIZE) as u64)
            .read_to_end(&mut data)
            .is_err()
        {
            return;
        }

        // Panic on parse errors so AFL can detect crashes.
        parse_transaction(&data).unwrap();
    }
}
