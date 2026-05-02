# Developers' guide

This guide captures the local developer workflow for the mxd Hotline server
project, with a focus on the commands required to format, lint, and test the
codebase, plus the PostgreSQL helper needed for integration coverage.

## Prerequisites

- Rust toolchain pinned by `rust-toolchain.toml`.
- `cargo` and `make` available on your `PATH`.
- Optional: `pg-embed-setup-unpriv` for PostgreSQL-backed tests.

### Build-tool resolution

The `Makefile` applies a conditional fallback for each build tool it invokes.
When the named tool is not found on `PATH`, the Makefile checks a fixed
well-known location and, if present, promotes it:

| Variable | Default | Fallback location |
| --- | --- | --- |
| `CARGO` | `cargo` | `~/.cargo/bin/cargo` |
| `WHITAKER` | `whitaker` | `~/.local/bin/whitaker` |
| `MDLINT` | `markdownlint-cli2` | `~/.bun/bin/markdownlint-cli2` |

This avoids silent failures when a tool is installed outside `PATH` and prevents
the Makefile from inadvertently resolving to an unintended binary earlier in
`PATH`. Override any variable at invocation time if the tool lives elsewhere:

```sh
make lint WHITAKER=/opt/custom/bin/whitaker
```

The fallback is a one-time check at parse time; it does not introduce a runtime
dependency on shell availability.


### Makefile PATH handling via `TOOL_PATH_PREFIX`

`TOOL_PATH_PREFIX` is built from the resolved Cargo binary directory, the
resolved Whitaker binary directory, and `~/.local/bin`. The Makefile resolves
the executable token first, records the directory only when lookup succeeds, and
then joins the non-empty entries:

```make
TOOL_PATH_PREFIX := $(shell printf '%s\n' \
  "$(CARGO_BIN_DIR)" "$(WHITAKER_BIN_DIR)" "$(LOCAL_BIN_DIR)" \
  | awk 'NF { printf "%s%s", sep, $$0; sep=":" }')
```

The lint targets prepend this prefix only for Whitaker invocations:

```sh
PATH="$(TOOL_PATH_PREFIX)$(if $(TOOL_PATH_PREFIX),:)$$PATH" \
  RUSTFLAGS="-D warnings" \
  whitaker --all -- --no-default-features --features "postgres test-support legacy-networking"
```

This keeps the same Cargo executable family, the resolved Whitaker binary, and
user-local tools ahead of the ambient shell `PATH` while avoiding an empty
current-directory entry. The Clippy lines still use `$(CARGO)` directly; the
PATH override is specifically for Whitaker and tools it invokes as
subprocesses during lint runs. Test targets use the resolved `$(CARGO)` path
directly rather than rewriting `PATH`.

To inspect the effective prefix for a local shell, ask `make` to print it:

```sh
make -pn | grep '^TOOL_PATH_PREFIX :='
```

For a local lint invocation, the effective command shape is:

```sh
PATH="$TOOL_PATH_PREFIX:$PATH" RUSTFLAGS="-D warnings" \
  whitaker --all -- --features "sqlite test-support"
```

That prefix means Cargo subcommands installed under `~/.cargo/bin`, Whitaker
installed under its resolved directory, and user-local tools under
`~/.local/bin` are found before system defaults during the Whitaker lint pass.

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

## Validator toggles for pending flows

The `validator` crate now ships placeholder validators for wireframe flows that
are still being implemented on parallel branches. This lets feature branches
opt into the required validation early without forcing the main branch to fail
before the corresponding server functionality lands.

By default, the pending validators are disabled in `validator/validator.toml`:

```toml
[validators]
chat = false
file_download = false
```

Environment variables override the file:

- `MXD_VALIDATOR_CONFIG` points at an alternate config file.
- `MXD_VALIDATOR_ENABLE_CHAT=true|false` enables or disables the chat
  validator.
- `MXD_VALIDATOR_ENABLE_FILE_DOWNLOAD=true|false` enables or disables the
  file-download validator.

When a pending validator is disabled, the corresponding test prints a clear
skip message and exits successfully. When it is enabled before the underlying
feature has landed, the validator fails with an explicit "enabled but not
implemented yet" error. That makes the opt-in suitable for parallel feature
branches that want the validation to go red until the branch completes the flow.

Examples:

```sh
# Install the pinned SynHX client once for local validator runs.
export HX_BIN_DIR="$HOME/.local/bin"
./scripts/install-synhx.sh

# Choose one way to expose the installed `hx` binary to later commands.
export PATH="$HX_BIN_DIR:$PATH"
# or
export MXD_VALIDATOR_HX_BINARY="$HX_BIN_DIR/hx"

# Run the supported sqlite validator suite against a prebuilt wireframe server.
make test-validator-sqlite

# Enable the chat validator for the current shell only.
export MXD_VALIDATOR_ENABLE_CHAT=true
cargo test -p validator --test pending_validators

# Use an alternate config file that enables both pending validators.
cat > /tmp/validator.toml <<'EOF'
[validators]
chat = true
file_download = true
EOF
export MXD_VALIDATOR_CONFIG=/tmp/validator.toml
cargo test -p validator --test pending_validators
```

The shared validator harness now resolves prerequisites explicitly:

- `make validator-sqlite-server` builds `target/debug/mxd-wireframe-server`.
- `make validator-postgres-server` builds
  `target/postgres/debug/mxd-wireframe-server`.
- `MXD_VALIDATOR_SERVER_BINARY` overrides the server binary path if the default
  target location is not suitable.
- `MXD_VALIDATOR_HX_BINARY` overrides the `hx` binary path.
- `MXD_VALIDATOR_FAIL_CLOSED=true|false` forces missing prerequisites to fail
  or skip regardless of whether `CI=true`.

Keep implemented validators such as login and XOR login coverage always on. The
pending toggles exist only for flows whose server-side work is still in
progress. At present, chat and file download remain pending, while SynHX file
listing and news posting are still blocked by client/server protocol-shape
differences rather than missing harness plumbing.

## Validator harness architecture

The `validator` crate is structured into five focused modules. Tests in
`validator/tests/` import primitives from `validator/src/lib.rs`, which
re-exports the public surface of each module.

### Module responsibilities

- `config.rs`: loads `validator.toml` and applies environment-variable
  overrides to determine which pending validators are enabled.
- `policy.rs`: decides whether a missing prerequisite causes a hard test
  failure (`fail_closed = true`) or a graceful skip (`fail_closed = false`).
  Reads `MXD_VALIDATOR_FAIL_CLOSED` and falls back to `CI=true` detection.
- `server_binary.rs`: resolves the path to a prebuilt
  `mxd-wireframe-server` binary. Precedence is
  `MXD_VALIDATOR_SERVER_BINARY`, `CARGO_BIN_EXE_mxd-wireframe-server`, then
  workspace `target/` candidates.
- `hx_client.rs`: discovers the `hx` binary, rejecting the Helix editor via a
  version probe. Spawns a PTY session via `expectrl` and provides helpers to
  wait for the Hotline prompt and terminate the session.
- `harness.rs`: orchestrates these pieces by running policy and prerequisite
  checks via `ValidatorHarness::prepare()`, launching the wireframe server
  with `start_server_with_setup()`, opening the PTY client with `spawn_hx()`,
  and exporting PTY expect/send helpers used directly by tests.

### Key public types

```rust
/// Central harness handle. Obtain one with `ValidatorHarness::default()` then
/// call `prepare()` before any other method.
pub struct ValidatorHarness { /* ... */ }

/// Describes whether a missing prerequisite should fail the test or skip it.
pub enum PrerequisiteResolution {
    Fail(String),
    Skip(String),
}

/// Errors returned when `hx` cannot be resolved or the PTY session fails.
pub enum HxClientError { /* ... */ }

/// Errors returned when no prebuilt wireframe server binary can be found.
pub enum ServerBinaryError { /* ... */ }
```

### Typical test structure

```rust
#[test]
fn my_validator_test() -> Result<(), AnyError> {
    let harness = ValidatorHarness::default();
    let Some(harness) = harness.prepare()? else {
        // Prerequisite missing and policy says skip.
        return Ok(());
    };
    let server = harness.start_server_with_setup(setup_login_db)?;
    let mut hx = harness.spawn_hx()?;
    send_line_and_expect(&mut hx, "/server -l alice -p secret 127.0.0.1 …", "…")?;
    expect_output_with_timeout(&mut hx, "connected", connect_expect_timeout())?;
    close_hx(&mut hx);
    Ok(())
}
```

`prepare()` returns `Ok(None)` when prerequisites are absent and the policy is
`fail_closed = false` (local developer environment). Tests must propagate the
`None` case as a skip rather than a panic.

### Payload-handling methods on `TransactionType`

Two const methods control how the wireframe layer handles request payloads:

- `rejects_payload(self, payload_is_empty: bool) -> bool` returns `true` when a
  non-empty payload should be rejected as invalid. Always returns `false` for
  `GetFileNameList` so directory context payloads pass through.
- `bypass_payload_decode(self) -> bool` returns `true` when the parameter block
  decoder should be skipped entirely and the raw bytes preserved. Used for
  `GetFileNameList` and any transaction type that does not accept a structured
  payload.

These methods replace the prior ad-hoc `!allows_payload()` checks in
`src/commands/mod.rs`, `src/wireframe/compat_layer.rs`, and
`src/transaction/params.rs`.

## Presence Runtime

Presence state is exposed through the stable crate-level API
`mxd::{PresenceRegistry, PresenceSnapshot, SessionPhase}`. The internal
`presence` module remains private so transport-specific helper functions do not
become part of the public crate surface.

`SessionPhase` controls when a snapshot is eligible for roster publication:

- `Unauthenticated` means no account has completed login.
- `PendingAgreement` means authentication succeeded, but Agreement Acceptance
  (121) still needs to complete before the session becomes visible.
- `Online` means the session may appear in Get User Name List (300) replies and
  may trigger Notify Change User (301) or Notify Delete User (302) traffic.

Sessions granted `NO_AGREEMENT` transition directly to `Online` at login.
Agreement-gated sessions stay in `PendingAgreement` until the agreement flow
finalizes. Only `Online` sessions should participate in the presence registry.

`PresenceSnapshot` is the transport-agnostic value published for an online
session. It carries `connection_id`, `user_id`, `display_name`, `icon_id`, and
`status_flags`. Adapter code supplies the connection identifier, then calls
`Session::presence_snapshot()` to combine that identifier with session state.
The snapshot is validated before insertion so field 300 replies and
notifications cannot contain unencodable user identifiers or display names.

`PresenceRegistry` stores online snapshots by outbound connection identifier.
`upsert` inserts or replaces a snapshot and returns the peer connection IDs that
should receive a change notification. `remove` deletes a snapshot by connection
ID and returns the removed snapshot plus the remaining peer IDs.
`online_snapshots` returns all online snapshots in deterministic order.
`snapshot_for_user_id` looks up a visible user and, when multiple sessions share
the same account user ID, selects the snapshot with the lowest connection ID.

The presence transaction builders convert snapshots into protocol replies and
server pushes. `build_user_name_list_reply` produces Get User Name List (300)
replies with repeated field-300 records. `build_notify_change_user` produces
Notify Change User (301) notifications. `build_notify_delete_user` produces
Notify Delete User (302) notifications. `build_client_info_text_reply`
produces Get Client Info Text (303) replies with the visible name and
placeholder info text.

### Error-handling conventions

- Both `HxClientError` and `ServerBinaryError` implement `std::error::Error`
  via `thiserror`.
- `ValidatorHarness::prepare()` maps these errors through `ValidatorRunPolicy`
  and either returns them as `anyhow::Error` (fail-closed) or returns
  `Ok(None)` (skip).
- PTY expect helpers (`expect_output`, `expect_output_with_timeout`,
  `expect_no_match`) embed the pending terminal output in the error message to
  simplify debugging failed assertions.
- `close_hx()` demotes session-cleanup errors to stderr diagnostics rather than
  failing the test, consistent with best-effort teardown.

## News schema alignment maintenance

Roadmap item 4.1.1 aligned the implemented news storage schema with
`docs/news-schema.md` using additive migrations rather than by rewriting
historical migration directories.

- Keep the SQLite and PostgreSQL migration trees in lock-step with the same
  version number and equivalent semantics.
- When a news schema change requires scoped uniqueness changes or defaulted
  timestamp columns, prefer explicit SQLite table rebuilds with copy-forward
  over incremental `ALTER TABLE` drift. The `00000000000007_align_news_schema`
  migration is the reference pattern.
- Preserve stable primary keys during copy-forward migrations so existing
  threaded article links and bundle/category relationships survive upgrades.
- Treat bundle/category GUID backfill and category serial-counter backfill as
  migration concerns when legacy rows must become structurally complete
  immediately after upgrade.
- Keep `permissions` and `user_permissions` schema work separate from runtime
  privilege loading and catalogue seeding. Schema alignment belongs to 4.1.1;
  enforcement and seed data belong to later roadmap items.
- Validate news schema changes with the backend-specific migration regression
  tests in `src/db/schema_alignment_tests/` and with the routing behaviour
  scenarios that exercise migrated databases.


### Schema alignment test harness (`src/db/schema_alignment_tests/`)

The schema-alignment tests are split by shared helpers and backend-specific
behaviour:

- `mod.rs`: shared migration runners, seed helpers, backfill assertions, and
  read-only backfill verifiers used by both backends.
- `sqlite_tests/`: isolated in-memory SQLite setup, legacy-schema rebuilds,
  catalogue checks through SQLite PRAGMA queries, GUID/counter tests, and
  SQLite-specific threading behaviour tests split into focused submodules.
- `postgres_tests/mod.rs`: PostgreSQL test entry points for fresh migration,
  legacy upgrade, scoped category uniqueness, and GUID behaviour.
- `postgres_tests/catalogue_helpers.rs`: PostgreSQL catalogue readers and
  assertions for tables, columns, indexes, constraints, permissions, and the
  database harness.
- `postgres_tests/threading.rs`: article-threading behaviour tests for
  self-referential `news_articles` links.

The shared helper surface is intentionally split between writers and readers:

- `run_statements` executes a sequence of SQL statements in order.
- `run_sql_script` splits migration SQL into individual statements before
  execution.
- `assert_upgrade_backfills` performs read-only bundle, category, permission,
  and article-index checks after an upgrade.
- `verify_root_category_names_are_unique_with_constraint_insert` performs an
  insert-based constraint verification and is mutation-driven by design.
- `seed_permission_round_trip` inserts the user, permission, and join rows used
  by permission smoke tests; it is the write path.
- `assert_permission_join_count` is the read-only assertion that checks the
  seeded permission join.

SQLite tests run against a fresh in-memory database per test or fixture.
PostgreSQL tests run through `with_postgres_test_db`, which creates an isolated
database using `POSTGRES_TEST_URL` when supplied or an embedded PostgreSQL
cluster otherwise. PostgreSQL tests use
`serial_test::file_serial(postgres_embedded_setup)` locks so the embedded
cluster setup and teardown are not raced by concurrent tests.

Run only the SQLite schema-alignment tests with:

```sh
RUSTFLAGS="-D warnings" \
  cargo nextest run --features "sqlite test-support" \
  db::schema_alignment_tests::sqlite_tests
```

Run only the PostgreSQL schema-alignment tests with:

```sh
RUSTFLAGS="-D warnings" \
  cargo nextest run --no-default-features \
  --features "postgres test-support legacy-networking" \
  db::schema_alignment_tests::postgres_tests
```

### News model metadata semantics

The aligned news schema adds metadata fields that make legacy rows structurally
complete after migration:

- `guid`: stable external identifier for bundles and categories. It is generated
  or backfilled during migration, non-empty at rest, and unique per row. Fresh
  inserts use database defaults or write-model values when provided.
- `created_at`: creation timestamp for bundles and categories. It is non-null
  at rest after migration and is backfilled for legacy rows at migration time.
  Write models keep the field optional for inserts until runtime enforcement
  work lands, so callers may rely on the database timestamp default.
- `add_sn`: category add serial number. During migration it is initialized from
  the article count that exists for the category at that moment. No trigger
  increments it when later articles are inserted, so fresh inserts do not
  auto-increment `add_sn`.
- `delete_sn`: category delete serial number. The migration initializes it to
  zero for existing categories and new rows rely on schema/write-model defaults
  unless an explicit value is supplied. No trigger updates it automatically.

Tests should distinguish migration-time backfill semantics from runtime insert
semantics. A fresh database category can have `add_sn = 0` even after articles
are inserted in the same test because no trigger updates the field.

Diesel joinables are defined for bundle, category, and article relationships so
query code can traverse `news_bundles`, `news_categories`, and `news_articles`
without relying on ad-hoc SQL joins for the common schema edges.

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
- `MissingPeerAddress` when the handshake metadata exists, but no peer address
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

WireframeServer construction now relies on the `AppFactory` trait rather than a
plain `Fn() -> WireframeApp` assumption. Closures still work through blanket
implementations, but migration work should make the return type explicit:

```rust,no_run
let server = WireframeServer::new(|| WireframeApp::default());
```

becomes:

```rust,no_run
let server = WireframeServer::new(|| -> Result<HotlineApp, AppFactoryError> {
    build_app_for_connection(&pool, &argon2, &outbound_registry)
});
```

Treat this as the preferred migration pattern whenever the adapter needs
connection-scoped handshake state or any other fallible setup at factory time.

Wireframe v0.3.0 also changed the codec and import surface that this adapter
uses:

- Add `wireframe = "0.3.0"` to `Cargo.toml`, enabling feature flags such as
  `testkit` explicitly when the main crate APIs are needed during tests or
  harness setup.
- `FrameCodec::wrap_payload` now takes `Bytes` rather than `Vec<u8>`. Codecs
  that still materialize owned frames can convert with `.to_vec()`, while
  zero-copy codecs should store the `Bytes` directly and optionally override
  `frame_payload_bytes(...)`.
- Root-level re-exports are no longer the stable import path for most adapter
  integrations. Prefer module paths such as `wireframe::app::WireframeApp`,
  `wireframe::codec::FrameCodec`, `wireframe::server::WireframeServer`,
  `wireframe::hooks::ConnectionContext`, and `wireframe::testkit::...`.
- Migration from older imports is mostly mechanical:

```rust,no_run
use bytes::Bytes;
use wireframe::{
    app::{Envelope, WireframeApp},
    codec::FrameCodec,
    hooks::ConnectionContext,
    server::WireframeServer,
};

impl FrameCodec for HotlineFrameCodec {
    type Frame = Vec<u8>;
    // ...

    fn wrap_payload(&self, payload: Bytes) -> Self::Frame { payload.to_vec() }
}
```

Keep these import-path changes explicit in migration patches so reviews can
confirm whether a call site still depends on a compatibility re-export or has
been moved onto the intended v0.3.0 module path.

## Database module

### Hierarchical path traversal (`src/db/file_path.rs`)

The `file_path` module provides backend-agnostic helpers for resolving
slash-delimited paths through the `file_nodes` hierarchy using recursive Common
Table Expressions (CTEs) via `diesel-cte-ext`.

Symbols:

- `CTE_SEED_SQL` (constant): seed row `(idx=0, id=NULL)` that anchors each
  traversal.
- `FILE_NODE_STEP_SQL` (constant): backend-specific recursive step. Postgres
  uses `json_array_elements_text ... WITH ORDINALITY`, while SQLite uses
  `json_each`.
- `FILE_NODE_BODY_SQL` (constant): terminal select that picks the node whose
  depth matches the segment count.
- `prepare_path` (function): normalizes a path string, trims leading and
  trailing slashes, and serializes segments as a JSON array alongside the
  segment count. Returns `None` for root-only paths. Returns
  `FileNodeLookupError::InvalidPath` for paths containing empty interior
  segments, such as `/Docs//guide.txt`.
- `build_path_cte` (function): constructs the full
  `WITH RECURSIVE tree ...` query from seed, step, and body fragments.
- `build_path_cte_with_conn` (function): convenience wrapper that infers the
  backend type from a `&mut C` connection parameter.

Callers pair `prepare_path` with `build_path_cte_with_conn`: `prepare_path`
validates and serializes the input; `build_path_cte_with_conn` builds the
parameterized CTE that the Diesel query then drives.

### File-node repository API (`src/db/files.rs`)

The following functions are re-exported from `src/db/mod.rs`:

- `create_file_node`: inserts a new file, folder, or alias node and returns
  the generated ID.
- `get_file_node`: fetches a single node by ID.
- `list_child_file_nodes`: lists all direct children of a folder node.
- `list_visible_root_file_nodes_for_user`: returns root nodes visible to a
  user via direct or group `resource_permissions`, merged with legacy `files`
  and `file_acl` rows.
- `resolve_file_node_path`: walks a slash-delimited path through the CTE and
  returns the terminal node.
- `resolve_alias_target`: follows an alias node to its target file node.
- `create_group`: inserts a principal group, idempotent by name.
- `add_user_to_group`: assigns a user to a group.
- `seed_permission`: inserts a permission catalogue row, idempotent by code.
- `grant_resource_permission`: attaches a permission grant to a `file_node`
  resource for a user or group principal.
- `download_file_permission`: returns the `NewPermission` descriptor for the
  canonical `download_file` entry (code 2).

`resolve_file_node_path` returns `Result<Option<FileNode>, FileNodeLookupError>`.
`FileNodeLookupError` has three variants:

- `InvalidPath`: invalid or malformed path.
- `Diesel(diesel::result::Error)`: a database query error.
- `Serde(serde_json::Error)`: a JSON serialization error during path
  preparation.

### Migration timeout (`src/db/migrations.rs`)

The `AppConfig` struct exposes a `migration_timeout_secs: Option<u64>` field,
set via the `--migration-timeout-secs` CLI flag or the
`MXD_MIGRATION_TIMEOUT_SECS` environment variable.

Behaviour:

- When unset or set to `0`, the built-in default of **15 seconds** is used.
- Positive values override the watchdog duration directly.
- On expiry, `run_with_migration_timeout` cancels the in-progress migration
  loop via a `CancellationToken` and returns a `SerializationError` wrapping
  `MigrationTimeoutError(duration)`.

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

## Presence runtime

The presence runtime is the in-memory authority for which users are currently
online. It lives in `src/presence.rs` and is threaded through the wireframe
server via `Arc<PresenceRegistry>`.

### `SessionPhase`

`SessionPhase` records whether a connection can participate in presence:

- `Unauthenticated`: the connection is established, but no credentials have
  been verified.
- `PendingAgreement`: login credentials have been accepted, but the user must
  still send the Agreed transaction (121) before becoming visible to peers.
- `Online`: the agreement has been accepted, or bypassed via `NO_AGREEMENT`;
  the user is visible in the roster and receives presence notifications.

Only `Online` sessions are included in the presence registry. Sessions in
`Unauthenticated` or `PendingAgreement` remain absent from roster replies and
presence fan-out.

### `PresenceSnapshot`

`PresenceSnapshot` is the transport-facing presence record built from an online
session. It carries:

- `connection_id: OutboundConnectionId`: the unique per-connection handle used
  for notification fan-out and registry removal.
- `user_id: i32`: the authenticated user's database identifier.
- `display_name: String`: the visible nickname, defaulting to the account
  username.
- `icon_id: u16`: the client-selected icon index, defaulting to 0.
- `status_flags: u16`: packed presence flags such as admin, away, and
  refuse-private-message state.

Build snapshots from a `Session` with `Session::presence_snapshot()`. The
method returns `None` unless the session phase is `Online`, keeping
agreement-gated users out of the roster until they complete the login
lifecycle.

### `PresenceRegistry`

`PresenceRegistry` owns the online snapshot set and exposes deterministic query
and mutation operations:

- `upsert(snapshot) -> Vec<OutboundConnectionId>` inserts or replaces the
  snapshot keyed by `connection_id`. It returns the connection IDs of all other
  registered peers so the caller can fan out a `301 Notify Change User`
  notification.
- `remove(connection_id) -> Option<PresenceRemoval>` removes the entry for the
  given connection. It returns a `PresenceRemoval` containing the departed
  snapshot and the remaining peer connection IDs for `302 Notify Delete User`
  fan-out, or `None` when the connection was not registered.
- `online_snapshots() -> Vec<PresenceSnapshot>` returns all registered
  snapshots in deterministic ascending `connection_id` order. Roster replies
  use this to build the `300 Get User Name List` response.
- `snapshot_for_user_id(user_id) -> Option<PresenceSnapshot>` returns the
  snapshot for the given database user ID. If duplicate sessions share the same
  user ID, the snapshot with the numerically lowest `connection_id` is
  returned.

### Transaction builders

The presence module also exposes builders for the roster and notification
transactions:

- `build_user_name_list_reply(header, snapshots)` produces transaction 300.
- `build_notify_change_user(snapshot)` produces transaction 301.
- `build_notify_delete_user(user_id)` produces transaction 302.
- `build_client_info_text_reply(header, display_name, info_text)` produces
  transaction 303.
