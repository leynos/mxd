# Adopt rstest-bdd v0.5.0 across behavioural suites

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

No `PLANS.md` was found in the repository root.

## Purpose / big picture

Upgrade the behavioural test stack from mixed `rstest-bdd` versions to
`v0.5.0`, remove repetitive scenario boilerplate, and adopt async-native step
execution where tests currently rely on per-world runtimes. Success means the
behavioural suites still assert the same outcomes, while `make check-fmt`,
`make lint`, and `make test` pass.

## Constraints

- Preserve existing behavioural intent in all `.feature` files.
- Keep test isolation per scenario and avoid cross-scenario mutable state.
- Avoid file-wide lint suppressions for scenario glue.
- Keep dependency changes limited to `rstest-bdd` and `rstest-bdd-macros`.
- Validate all changes through Makefile gates.

## Tolerances (exception triggers)

- Scope: stop if migration requires more than 30 files or 1500 net LOC.
- Interface: stop if non-test public APIs must change.
- Dependency: stop if additional third-party crates are required.
- Validation: stop if repeated gate failures provide no new diagnostics.
- Ambiguity: stop if behavioural semantics would need to change.

## Risks

- Risk: async scenario migration can trigger nested-runtime panics when setup
  helpers call `Runtime::block_on`. Severity: medium. Likelihood: high.
  Mitigation: route setup through async-safe helpers and `spawn_blocking`
  boundaries where synchronous setup is unavoidable.
- Risk: replacing manual `#[scenario]` stubs with `scenarios!` can alter fixture
  name mapping. Severity: medium. Likelihood: medium. Mitigation: verify
  fixture names and use explicit remapping (`#[from(...)]`) when
  underscore-prefixed parameters are required.

## Progress

- [x] (2026-02-06) Confirmed branch and gathered migration context from
  `docs/rstest-bdd-v0-5-0-migration-guide.md` and
  `docs/rstest-bdd-users-guide.md`.
- [x] (2026-02-06) Upgraded `rstest-bdd` dependencies to `v0.5.0` in root and
  `crates/mxd-verification`.
- [x] (2026-02-06) Replaced repeated manual scenario stubs with `scenarios!`
  bindings in behavioural suites.
- [x] (2026-02-06) Migrated async-sensitive suites to async step execution
  where fixture setup stays runtime-safe.
- [x] (2026-02-06) Kept embedded-Postgres-backed suites on synchronous step
  glue with fixture-owned runtimes to avoid nested runtime panics.
- [x] (2026-02-06) Added async-safe database setup path in
  `test-util/src/bdd_helpers.rs`.
- [x] (2026-02-06) Removed file-wide lint suppressions in BDD scenario files.
- [x] (2026-02-06) Updated `docs/developers-guide.md` with current behavioural
  testing strategy.
- [x] (2026-02-06) Ran `make check-fmt`, `make lint`, and `make test` to green.

## Surprises & Discoveries

- Observation: embedded `PostgreSQL` fixture guards are not `Send`, so a full
  database setup cannot be moved to a worker thread and returned to async
  scenario code. Evidence: `E0277`
  (`std::rc::Rc<()> cannot be sent between threads safely`) when attempting to
  return `Result<Option<TestDb>, AnyError>` across a thread channel. Impact:
  adopted a split strategy: async scenarios where safe, synchronous scenarios
  for embedded-Postgres setup paths.
- Observation: underscore-prefixed fixture parameter names are treated as
  distinct fixture keys unless explicitly remapped. Evidence:
  `tests/runtime_selection_bdd.rs` and `tests/wireframe_routing_bdd.rs` step
  resolution failures for missing `_world` fixture. Impact: used
  `#[from(world)] _world` where underscore naming is desired.

## Decision Log

- Decision: migrate both root behavioural suites and
  `crates/mxd-verification` to a single `rstest-bdd` version. Rationale: avoid
  split behaviour/tooling semantics. Date/Author: 2026-02-06 / Codex.
- Decision: prefer `scenarios!` per feature file over repeated indexed
  `#[scenario]` stubs. Rationale: less boilerplate and fewer unused-fixture
  sink statements. Date/Author: 2026-02-06 / Codex.
- Decision: convert async-sensitive suites to async steps under
  `runtime = "tokio-current-thread"`. Rationale: removes world-local runtimes
  and aligns with v0.5.0 guidance. Date/Author: 2026-02-06 / Codex.
- Decision: for suites that rely on embedded `PostgreSQL` setup helpers,
  preserve synchronous step glue with fixture-owned runtimes. Rationale:
  prevents nested runtime panics and avoids crossing thread boundaries with
  non-`Send` `PostgresTestDb` guards. Date/Author: 2026-02-06 / Codex.

## Outcomes & Retrospective

Migration completed with all quality gates passing. Behavioural suites now use
`rstest-bdd` v0.5.0 with `scenarios!` bindings, no file-wide scenario lint
suppressions, and underscore fixture remapping where needed. The final strategy
uses async steps for runtime-safe suites and synchronous step glue for suites
that bootstrap embedded `PostgreSQL`.

## Context and orientation

Primary files changed:

- `Cargo.toml`
- `crates/mxd-verification/Cargo.toml`
- `test-util/src/bdd_helpers.rs`
- `test-util/src/lib.rs`
- `tests/create_user_bdd.rs`
- `tests/outbound_messaging_bdd.rs`
- `tests/runtime_selection_bdd.rs`
- `tests/session_privileges_bdd.rs`
- `tests/transaction_streaming.rs`
- `tests/wireframe_handshake_metadata.rs`
- `tests/wireframe_login_compat.rs`
- `tests/wireframe_routing_bdd.rs`
- `tests/wireframe_transaction.rs`
- `tests/wireframe_transaction_encoding.rs`
- `tests/wireframe_xor_compat.rs`
- `crates/mxd-verification/tests/session_gating_bdd.rs`
- `docs/developers-guide.md`

## Plan of work

Stage A upgraded dependencies and checked targeted behavioural suites.

Stage B replaced repeated scenario function stubs with `scenarios!` bindings
and fixture injection.

Stage C migrated async-sensitive steps to `async fn` handlers and removed
per-world runtime fields.

Stage D updated documentation to reflect the implemented strategy.

Stage E runs full quality gates and finalises commits.

## Concrete steps

From repository root:

1. `make check-fmt | tee /tmp/check-fmt-$(basename "$(pwd)")-$(git branch --show).out`
2. `make lint | tee /tmp/lint-$(basename "$(pwd)")-$(git branch --show).out`
3. `make test | tee /tmp/test-$(basename "$(pwd)")-$(git branch --show).out`

Targeted verification (already used during migration):

- `cargo test --test outbound_messaging_bdd --no-default-features`
  `--features "sqlite test-support"`
- `cargo test --test session_privileges_bdd --no-default-features`
  `--features "sqlite test-support"`
- `cargo test --test wireframe_routing_bdd --no-default-features`
  `--features "sqlite test-support"`
- `cargo test --test wireframe_handshake_metadata --no-default-features`
  `--features "sqlite test-support"`
- `cargo test --test wireframe_transaction_encoding --no-default-features`
  `--features "sqlite test-support"`
- `cargo test -p mxd-verification --test session_gating_bdd`

## Validation and acceptance

Acceptance criteria:

- Workspace behavioural crates use `rstest-bdd`/`rstest-bdd-macros` v0.5.0.
- Async-sensitive behavioural suites use async steps where runtime-safe.
- Embedded-Postgres behavioural suites use synchronous step glue to keep setup
  out of nested runtime contexts.
- Repeated scenario stubs are replaced by `scenarios!` in migrated suites.
- BDD scenario files avoid file-wide lint suppressions.
- `make check-fmt`, `make lint`, and `make test` pass.

## Idempotence and recovery

Each migration step is re-runnable. If a suite fails after conversion, rerun
the targeted test command for that suite, revert only the affected file, and
reapply the migration incrementally.

## Artifacts and notes

Observed nested-runtime failure during migration:

    Cannot start a runtime from within a runtime

Observed fixture-name mismatch when using underscore-prefixed fixture parameter
without remapping:

    requires fixtures _world … Available fixtures … world

## Interfaces and dependencies

Updated dependency interfaces:

    rstest-bdd = "0.5.0"
    rstest-bdd-macros = { version = "0.5.0", features = ["compile-time-validation"] }

New helper in `test-util`:

    pub async fn build_test_db_async(setup: SetupFn) -> Result<Option<TestDb>, AnyError>

## Revision note

Initial plan drafted from migration docs and existing suite inventory, then
updated during implementation to record fixture-name remapping, runtime
constraints around embedded `PostgreSQL`, and the final split async/sync
behavioural strategy.
