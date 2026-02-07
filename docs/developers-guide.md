# Developers' guide

This guide captures the local developer workflow for the mxd Hotline server
project, with a focus on the commands required to format, lint, and test the
codebase, plus the PostgreSQL helper needed for integration coverage.

## Prerequisites

- Rust toolchain pinned by `rust-toolchain.toml`.
- `cargo` and `make` available on your `PATH`.
- Optional: `pg-embed-setup-unpriv` for PostgreSQL-backed tests.

## PostgreSQL test helper

Install the helper once:

```sh
cargo install pg-embed-setup-unpriv
```

Run the helper before `make test` whenever PostgreSQL coverage is required. The
helper runs unprivileged; root access is not required.

```sh
export PG_VERSION_REQ="=16.4.0"
export PG_RUNTIME_DIR="/var/tmp/pg-embedded-setup-unpriv/install"
export PG_DATA_DIR="/var/tmp/pg-embedded-setup-unpriv/data"
export PG_SUPERUSER="postgres"
export PG_PASSWORD="postgres_pass"
pg_embedded_setup_unpriv
```

See `docs/pg-embed-setup-unpriv-users-guide.md` for the full reference and
troubleshooting tips.

## Behavioural testing strategy

The behavioural suite uses `rstest-bdd` v0.5.0 in both the root crate and
`crates/mxd-verification`.

- Prefer `scenarios!` bindings to a specific `.feature` file rather than manual
  repeated `#[scenario(index = ...)]` stubs.
- Use `fixtures = [name: Type]` on `scenarios!` so shared world fixtures are
  injected consistently into step definitions.
- Prefer async behavioural scenarios for async-sensitive suites:
  `runtime = "tokio-current-thread"` with `async fn` step handlers where async
  I/O is exercised and fixture setup does not rely on embedded PostgreSQL
  cluster bootstrapping.
- For suites that must initialize embedded PostgreSQL fixtures, keep step
  handlers synchronous and run async routing calls through a fixture-owned
  Tokio runtime to avoid nested runtime panics in PostgreSQL setup helpers.
- Keep scenario state isolated per scenario. Share only infrastructure with
  explicit fixture choices; do not depend on scenario execution order.
- If a manual `#[scenario]` binding must keep an intentionally unused fixture,
  use an underscore-prefixed parameter with explicit remapping, for example
  `#[from(world)] _world: RuntimeWorld`.
- Do not add file-wide lint suppressions for scenario glue. Scope lint
  expectations tightly to the smallest function or statement that requires them.
- If a scenario needs a fallible return signature, use explicit
  `Result<(), E>` or `StepResult<(), E>` in the scenario function signature.

## Quality gates

Run the full suite from the repository root after making changes:

```sh
make fmt
make markdownlint
make nixie
make check-fmt
make lint
make test
```
