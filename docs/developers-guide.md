# Developers' guide

This guide captures the local developer workflow for MXD, with a focus on the
commands required to format, lint, and test the codebase, plus the PostgreSQL
helper needed for integration coverage.

## Prerequisites

- Rust toolchain pinned by `rust-toolchain.toml`.
- `cargo` and `make` available on your `PATH`.
- Optional: `pg-embed-setup-unpriv` for PostgreSQL-backed tests.

## PostgreSQL test helper

Install the helper once:

```sh
cargo install pg-embed-setup-unpriv
```

Run the helper before `make test` whenever you want PostgreSQL coverage. The
helper runs unprivileged; root access is not required.

```sh
export PG_VERSION_REQ="=16.4.0"
export PG_RUNTIME_DIR="/var/tmp/pg-embedded-setup-unpriv/install"
export PG_DATA_DIR="/var/tmp/pg-embedded-setup-unpriv/data"
export PG_SUPERUSER="postgres"
export PG_PASSWORD="postgres_pass"

```pg_embedded_setup_unpriv

See `docs/pg-embed-setup-unpriv-users-guide.md` for the full reference and
troubleshooting tips.

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
