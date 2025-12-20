# User guide

This guide explains how to run both server binaries and how to use their shared
administrative subcommands. The command-line interface (CLI) definitions live
in `mxd::server::cli`, while the active networking runtime is selected by the
`legacy-networking` Cargo feature.

## Launching the legacy server

- Build the sqlite variant with `make sqlite` or the postgres variant with
  `make postgres`. The `legacy-networking` feature is enabled by default, so
  the `mxd` binary is available.
- Start the daemon with `cargo run --bin mxd -- --bind 0.0.0.0:5500 --database
  mxd.db`.
- Override `MXD_`-prefixed environment variables or drop-in `.mxd.toml` files
  to persist defaults; the server merges file and env layers before applying
  CLI overrides via `ortho-config`.
- The listener prints `mxd listening on …` once the Tokio accept loop is
  running. Use `Ctrl-C` (and `SIGTERM` on Unix) for an orderly shutdown.

## Launching the Wireframe server

- Build the sqlite variant with `make APP=mxd-wireframe-server sqlite` or the
  postgres variant with `make APP=mxd-wireframe-server postgres`. The targets
  reuse the shared CLI module, so both binaries honour the same flags,
  environment overrides, and `.mxd.toml` defaults.
- Start the daemon with `cargo run --bin mxd-wireframe-server -- --bind
  0.0.0.0:6600 --database mxd.db`. The binary prints `mxd-wireframe-server
  listening on …` after the Wireframe listener binds.
- Administrative subcommands such as `create-user` remain available because
  the bootstrap calls `mxd::server::run_command` before starting the listener.
- The Wireframe listener now decodes the Hotline 12-byte handshake preamble,
  uses the upstream preamble hooks to send the 8-byte reply (0 on success, 1
  for invalid protocol, 2 for unsupported version, 3 for handshake timeout),
  drops idle sockets after the five-second handshake timeout before routing,
  and records the negotiated sub-protocol ID and sub-version in per-connection
  state. The metadata stays available for the lifetime of the connection, so
  compatibility shims can branch on client quirks, and it is cleared during
  teardown to avoid leaking between sessions. The transaction framing codec is
  in place (including multi-fragment reassembly), but protocol routes remain
  pending, so behaviour beyond handshake is unchanged.

## Selecting a runtime

- Keep the legacy loop enabled (default) to run the classic `mxd` daemon.
- Disable it with `--no-default-features --features "sqlite toml"` to obtain a
  Wireframe-only build; the `mxd` binary is skipped via `required-features` and
  `server::run()` invokes the Wireframe runtime.
- `make test-wireframe-only` exercises the Wireframe-first configuration and
  runs the behaviour scenarios that assert the feature gate.

## Creating users

The `create-user` subcommand now runs entirely inside the library so that it is
available to every binary. Supply both `--username` and `--password`; missing
values produce the same `missing username`/`missing password` errors that the
new `rstest-bdd` scenarios cover. Example:

```sh
cargo run --bin mxd -- create-user --username alice --password secret
```

The command runs pending migrations before inserting the user. Errors bubble up
unchanged, so the shell exit code remains reliable in automation scripts.

## Testing against PostgreSQL

Integration tests and developer machines can exercise the postgres backend by
running `pg_embedded_setup_unpriv` before `make test`. The helper stages a
PostgreSQL distribution with unprivileged ownership, so the
`postgresql_embedded` crate can start temporary clusters without root access.
After invoking the helper, run `make test` to execute sqlite, postgres, and
wireframe-only suites; the postgres jobs automatically reuse the staged
binaries. Both server binaries honour the same `MXD_DATABASE` and `--database`
values, allowing the helper to be re-run once and then switching between
`cargo run --bin mxd` and `cargo run --bin mxd-wireframe-server` without
additional setup. Refer to `docs/pg-embedded-setup-unpriv-users-guide.md` for
cluster path customization and privilege troubleshooting details.

## Behaviour coverage

Unit tests for the CLI live next to `src/server/cli.rs` and rely on `rstest`
fixtures to validate configuration loading across env, dotfile, and CLI layers.
High-level behaviour tests are defined in `tests/features/create_user_command`
and `tests/features/runtime_selection.feature`, bound via `rstest-bdd`. They
cover successful account creation, missing-credential errors, and runtime
selection so the adapter strategy remains observable to end users.
