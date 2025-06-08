# mxd
Marrakesh Express Daemon

Hop aboard the Marrakesh Express — a compact but spirited
[Hotline](https://hotline.fandom.com/wiki/Virtual1%27s_Hotline_Server_Protocol_Guide)
server written in Rust. It speaks just enough of the protocol for that retro BBS
flair. The server uses Tokio for async networking and Diesel's async extension to
keep users safely stored in SQLite. Passwords are salted and hashed with Argon2,
whose knobs are adjustable via `--argon2-m-cost`, `--argon2-t-cost`, and
`--argon2-p-cost`.

Commands arrive line by line through a `BufReader`. At present only a `LOGIN`
command is supported; invalid attempts earn an `ERR` reply. Each session remains
open so multiple commands can be processed until the client disconnects.

Tokio keeps everything asynchronous, and this project aims to be the skeleton
for a more complete Hotline implementation. See the `docs/` directory for a dive
into the protocol and how we juggle SQLite and PostgreSQL migrations.

## Running

Build the project and run the daemon. Specify a bind address and database path if the defaults don't tickle your fancy:

```
cargo build

# Run server listening on the default address
cargo run -- --bind 0.0.0.0:5500 --database mxd.db
```

### Creating users

Use the `create-user` subcommand to add accounts:

```
cargo run -- create-user alice secret
```

### Running tests

```
cargo test
```

Integration tests live in the repository's `tests/` directory.


## Validation harness

The `validator` crate provides a compatibility check using the `shx` client and `expectrl` to ensure mxd speaks the Hotline protocol correctly. Install `shx` version 0.2.4 and make sure it's on your `PATH` before running:

```bash
cd validator
cargo test
```

## Fuzzing

A simple AFL++ harness lives in the `fuzz/` directory.

A `fuzz/Dockerfile` builds the debug harness with sanitizers and runs AFL++ in a container.

Crash files appear under `artifacts/main/crashes`.

```bash
# install afl++ and make sure afl-clang-fast is on your PATH
export CC=afl-clang-fast
export CXX=afl-clang-fast++
export RUSTFLAGS="-Zsanitizer=address"

# compile the instrumented binary
cargo afl build --manifest-path fuzz/Cargo.toml

# prepare a corpus directory of initial inputs
mkdir -p fuzz/corpus

# run the fuzzer
cargo afl fuzz -i fuzz/corpus -o findings fuzz/target/debug/fuzz
```

### Running in Docker

A `fuzz/Dockerfile` is provided to build the harness and run AFL++ in a container.

```bash
# build the fuzzing image
docker build -t mxd-fuzz -f fuzz/Dockerfile .

# run with your corpus and an output directory for results
mkdir -p fuzz/corpus artifacts
docker run --rm \
  -v $(pwd)/fuzz/corpus:/corpus \
  -v $(pwd)/artifacts:/out \
  mxd-fuzz
```

Crash files will appear under `artifacts/main/crashes`.

The `src/bin/gen_corpus.rs` utility rebuilds the seed files placed in
`fuzz/corpus/` from the transactions crafted in the integration tests.
Run it whenever you want to refresh or extend the corpus. A simple
`Makefile` provides a convenience target so you only need to run:

```bash
make corpus
```

Add new transactions to the tool to grow the set of starting inputs.
