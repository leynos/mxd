extern "C" { fn __AFL_LOOP(cnt: u32) -> i32; }
use std::io::{self, Read};

fn main() {
    let mut data = Vec::new();
    loop {
        if unsafe { __AFL_LOOP(1000) } == 0 { break; }
        data.clear();
        if io::stdin().read_to_end(&mut data).is_err() { return; }
        let _ = mxd::transaction::parse_transaction(&data);
    }
}
