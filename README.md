# mxd

Marrakesh Express Daemon

Hop aboard the Marrakesh Express — a compact but spirited
[Hotline](https://hotline.fandom.com/wiki/Virtual1%27s_Hotline_Server_Protocol_Guide)
 server written in Rust. It speaks just enough of the protocol for that retro
BBS flair. The server uses Tokio for async networking and Diesel's async
extension to keep users safely stored in SQLite. Passwords are salted and
hashed with Argon2, whose knobs are adjustable via `--argon2-m-cost`,
`--argon2-t-cost`, and `--argon2-p-cost`.

Commands arrive line by line through a `BufReader`. At present only a `LOGIN`
command is supported; invalid attempts earn an `ERR` reply. Each session
remains open so multiple commands can be processed until the client disconnects.

Tokio keeps everything asynchronous, and this project aims to be the skeleton
for a more complete Hotline implementation. See the `docs/` directory for a
dive into the protocol and how we juggle SQLite and PostgreSQL migrations.

If you use SQLite as the persistence engine, ensure your build of SQLite
includes the `JSON1` extension and support for recursive common table
expressions (CTEs). The server verifies these features on startup and will fail
to run if they are missing.

When using PostgreSQL, run against a server version **14 or newer**. The daemon
checks this at startup and refuses to launch on older versions.

mxd supports both SQLite and PostgreSQL backends. Select one at compile time
using `--features sqlite` or `--features postgres`. Exactly one of these
features must be enabled for a successful build. Because the `sqlite` feature
is enabled by default, you must disable default features when opting into
PostgreSQL:

```bash
cargo run --no-default-features --features postgres -- --help
```

The same applies to `cargo build` and `cargo test` commands targeting
PostgreSQL. **Note**: PostgreSQL backend support is currently a work in
progress.

## Running

Build the project and run the daemon. Specify a bind address and database *URL*
if the defaults don't tickle your fancy. For SQLite use a filesystem path,
while PostgreSQL requires a standard `postgres://` URL. Enable the appropriate
backend feature when compiling:

```bash

cargo build --features sqlite

# Run server listening on the default address
cargo run --features sqlite -- --bind 0.0.0.0:5500 --database mxd.db

# PostgreSQL example
# cargo run --no-default-features --features postgres -- --database postgres://user:pass@localhost/mxd
```

### Creating users

Use the `create-user` subcommand to add accounts:

```bash
cargo run --features sqlite -- create-user alice secret
```

### Managing migrations

Install the `diesel` CLI with both back-end features so you can run migrations
manually when needed:

```bash
cargo install diesel_cli --no-default-features --features sqlite,postgres
```

### Running tests

```bash
cargo test
```

Integration tests live in the repository's `tests/` directory.

When the `postgres` feature is enabled, tests normally spin up an embedded
PostgreSQL server. Set `POSTGRES_TEST_URL` to reuse an existing database URL

- Inject `postgres_db` into any test that needs Postgres.
- If `POSTGRES_TEST_URL` is set, the fixture uses that database; otherwise, it
  starts an embedded Postgres server.
- The `public` schema is dropped and recreated **before** each test and again on
  teardown, so every test runs against a pristine schema regardless of reuse.

## Validation harness

The `validator` crate provides a compatibility check using the `hx` client and
`expectrl` to ensure mxd speaks the Hotline protocol correctly. Install `hx`
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
