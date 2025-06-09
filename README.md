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

Build the project and run the daemon. Specify a bind address and
database path if the defaults don't tickle your fancy:

```bash
cargo build

# Run server listening on the default address
cargo run -- --bind 0.0.0.0:5500 --database mxd.db
```

### Creating users

Use the `create-user` subcommand to add accounts:

```bash
cargo run -- create-user alice secret
```

### Running tests

```bash
cargo test
```

Integration tests live in the repository's `tests/` directory.

## Validation harness

The `validator` crate provides a compatibility check using the `shx` client and
`expectrl` to ensure mxd speaks the Hotline protocol correctly. Install `shx`
version 0.2.4 and make sure it's on your `PATH` before running:

```bash
cd validator
cargo test
```

## Fuzzing

The repository includes an AFL++ harness under `fuzz/`. See
[docs/fuzzing.md](docs/fuzzing.md) for build commands, Docker usage and how the
nightly GitHub Actions job integrates AFL++. Crash files are written to
`artifacts/main/crashes`.
