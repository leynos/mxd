# User guide

This guide explains how to run the legacy Hotline server binary and how to use
its administrative subcommands. The CLI definitions live in the shared
`mxd::server::cli` module, so both the existing TCP binary and the upcoming
wireframe binary expose the same configuration surface.

## Launching the legacy server

- Build the sqlite variant with `make sqlite` or the postgres variant with
  `make postgres`. The binaries use the same CLI flags because they both call
  `mxd::server::run()`.
- Start the daemon with `cargo run --bin mxd -- --bind 0.0.0.0:5500 --database
  mxd.db`.
- Override `MXD_`-prefixed environment variables or drop-in `.mxd.toml` files
  to persist defaults; the server merges file and env layers before applying
  CLI overrides via `ortho-config`.
- The listener prints `mxd listening on …` once the Tokio accept loop is
  running. Use `Ctrl-C` (and `SIGTERM` on Unix) for an orderly shutdown.

## Creating users

The `create-user` subcommand now runs entirely inside the library so that it is
available to every binary. Supply both `--username` and `--password`; missing
values produce the same `missing username`/`missing password` errors that the
new `rstest-bdd` scenarios cover. Example:

```sh
cargo run --bin mxd -- create-user --username alice --password secret
```

The command runs pending migrations before inserting the user. Errors bubble up
unchanged, so you can rely on the shell exit code in automation scripts.

## Testing against PostgreSQL

Integration tests and developer machines can exercise the postgres backend by
running `pg_embedded_setup_unpriv` before `make test`. The helper stages a
PostgreSQL distribution with unprivileged ownership, so the
`postgresql_embedded` crate can start temporary clusters without root access.
After invoking the helper, run `make test` to execute both sqlite and postgres
suites; the postgres jobs automatically reuse the staged binaries.

## Behaviour coverage

Unit tests for the CLI live next to `src/server/cli.rs` and rely on `rstest`
fixtures to validate configuration loading across env, dotfile, and CLI layers.
High-level behaviour tests are defined in `tests/features/create_user_command`
and bound via `rstest-bdd`. They cover successful account creation and the
error path when credentials are omitted, ensuring the extracted command logic
remains observable to end users.
