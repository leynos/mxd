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
cargo install --locked pg-embed-setup-unpriv --version 0.5.0
```

Run the helper before `make test` whenever PostgreSQL coverage is required. The
helper runs unprivileged; root access is not required.

```sh
export PG_VERSION_REQ="=16.4.0"
export PG_RUNTIME_DIR="/var/tmp/pg-embedded-setup-unpriv/install"
export PG_DATA_DIR="/var/tmp/pg-embedded-setup-unpriv/data"
export PG_SUPERUSER="postgres"
export PG_PASSWORD="postgres_pass"
export PG_TEST_BACKEND="postgresql_embedded"
pg_embedded_setup_unpriv
```

`PG_TEST_BACKEND` accepts only unset or `postgresql_embedded` for embedded
cluster bootstrapping. Any other value should be treated as an intentional
skip/fail signal from the test harness.

See `docs/pg-embed-setup-unpriv-users-guide.md` for the full reference and
troubleshooting tips.

## PostgreSQL migration strategy (v0.5.0)

The migration target for this branch adopts v0.5.0 lifecycle APIs to improve
test reliability and throughput without changing test semantics.

- Keep `POSTGRES_TEST_URL` support for external PostgreSQL integration tests.
- Use template-based provisioning (`postgres_db_fast`) with a process-shared
  `ClusterHandle` and `CREATE DATABASE ... TEMPLATE` clones so migration
  amortization remains effective under v0.5.0 cleanup defaults.
- Use send-safe split lifecycle APIs (`TestCluster::new_split()` and
  `TestCluster::start_async_split()`) or
  `test_support::shared_cluster_handle()` when shared fixtures must cross
  thread or timeout boundaries.
- Prefer default cleanup (`CleanupMode::DataOnly`) for day-to-day runs, use
  `CleanupMode::Full` for strict filesystem hygiene, and reserve
  `CleanupMode::None` for explicit forensic debugging sessions.

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

## Wireframe adapter context handoff

The Wireframe adapter carries Hotline handshake metadata from the asynchronous
handshake hook into the synchronous app factory through task-local state plus a
task-ID keyed registry in `src/wireframe/connection.rs`.

- `scope_current_context(...)` seeds the per-task context for a future and
  mirrors any initial context into the registry so post-handshake app-factory
  code can retrieve it after the scoped future exits.
- `store_current_context(...)` updates both the task-local slot and the
  registry for the current Tokio task.
- `take_current_context()` consumes the context for the current task. The app
  factory uses this to fail closed once the per-connection state has been
  handed off.
- `has_current_context()` is the public visibility probe for code that needs
  to ask whether the current Tokio task can see stored context.

The Wireframe server bootstrap converts app-factory failures into the internal
`AppFactoryError` enum in `src/server/wireframe/mod.rs`. Current variants are:

- `MissingHandshakeContext` when no handshake metadata was stored for the
  current task.
- `MissingPeerAddress` when the handshake metadata exists but no peer address
  was attached.
- `BuildApplication` when the underlying `WireframeApp` builder returns an
  error while registering middleware or routes.

Use the fallible app-factory pattern when per-connection setup can fail:

```rust,no_run
fn app_factory() -> Result<HotlineApp, AppFactoryError> {
    build_app_for_connection(&pool, &argon2, &outbound_registry)
}
```

Returning `Result` allows the adapter to preserve typed failure information and
propagate setup errors without panicking. Keep these failures explicit in tests
so missing handshake metadata, missing peer metadata, and builder failures all
remain covered.

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
