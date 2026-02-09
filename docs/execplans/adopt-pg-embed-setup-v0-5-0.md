# Migrate Postgres tests to pg-embed-setup-unpriv v0.5.0

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: DRAFT

No `PLANS.md` was found in the repository root.

## Purpose / big picture

Upgrade this repository's Postgres-backed test stack from
`pg-embed-setup-unpriv` `v0.4.0` to `v0.5.0` and adopt the v0.5.0 APIs that
improve reliability, send-safety, and test throughput. After the change, the
suite should continue to support both embedded and external PostgreSQL test
backends, but embedded paths should use v0.5.0 capabilities for explicit
cleanup behaviour, send-safe shared fixtures, and template-based fast
provisioning.

Success is observable when:

- `Cargo.toml` and `test-util/Cargo.toml` pin `pg-embed-setup-unpriv` to
  `0.5.0`.
- Postgres test helpers in `test-util/src/postgres.rs` use the v0.5.0
  lifecycle model in at least one shared/send-safe path.
- `docs/developers-guide.md` describes the strategy actually used by the test
  suite after migration.
- `make check-fmt`, `make lint`, and `make test` pass.

## Constraints

- Keep behavioural parity for existing tests: no intentional semantic changes
  to test assertions, skip behaviour, or database isolation guarantees.
- Preserve support for `POSTGRES_TEST_URL` external database mode.
- Keep migration scope focused on Postgres test setup paths and related docs;
  do not change production database runtime logic.
- Do not add new third-party dependencies.
- Keep all validation on Makefile targets and capture long command output via
  `tee` logs.
- Keep Rust/clippy policy intact (no lint suppressions added solely to force
  this migration through).

If the migration requires violating any constraint, stop and escalate.

## Tolerances (exception triggers)

- Scope: stop if implementation needs more than 12 files or 700 net lines.
- Interface: stop if public non-test APIs must change outside `test-util`.
- Dependency: stop if any new crate beyond the `pg-embed-setup-unpriv` version
  bump is required.
- Validation: stop if the same quality gate fails twice without new
  diagnostics.
- Ambiguity: stop if two viable fixture lifecycle models remain and either
  would materially change teardown timing.
- Time: stop and escalate if a single milestone exceeds 2 hours elapsed.

## Risks

- Risk: `TestCluster` send-safety boundaries may require reshaping fixtures and
  internal helper ownership in `test-util/src/postgres.rs`. Severity: medium.
  Likelihood: high. Mitigation: adopt v0.5.0 handle/guard split (`new_split` or
  `shared_test_cluster_handle`) in shared paths and keep ownership explicit.

- Risk: new `PG_TEST_BACKEND` validation can turn previously permissive CI
  configurations into skip/error paths. Severity: medium. Likelihood: medium.
  Mitigation: add/adjust test coverage to assert accepted values and document
  expected environment settings in `docs/developers-guide.md`.

- Risk: cleanup defaults (`CleanupMode::DataOnly`) may interfere with any
  forensic debugging workflow that expects directories to persist. Severity:
  low. Likelihood: medium. Mitigation: keep default for normal tests, document
  explicit `CleanupMode::None`/`Full` usage for debugging and CI hygiene.

- Risk: template-based provisioning can create hidden coupling if template
  invalidation is weak. Severity: medium. Likelihood: medium. Mitigation:
  retain hash-based template naming (`hash_directory("migrations")`) and verify
  clone setup with a migration-change-sensitive name.

## Progress

- [x] (2026-02-09 02:25Z) Confirmed branch and read migration context in
  `docs/pg-embed-setup-unpriv-v0-5-0-migration-guide.md` and
  `docs/pg-embed-setup-unpriv-users-guide.md`.
- [x] (2026-02-09 02:27Z) Mapped current Postgres test integration points:
  `Cargo.toml`, `test-util/Cargo.toml`, `test-util/src/postgres.rs`,
  `tests/common.rs`, `tests/postgres_env.rs`, and `docs/developers-guide.md`.
- [x] (2026-02-09 02:30Z) Drafted migration plan with v0.5.0 feature adoption,
  constraints, tolerances, validation, and commit sequencing.
- [x] (2026-02-09 02:35Z) Updated `docs/developers-guide.md` so the documented
  strategy and helper usage are cohesive with the v0.5.0 migration target.
- [x] (2026-02-09 02:35Z) Ran quality gates for this planning/documentation
  change: `make check-fmt`, `make lint`, and `make test`.
- [ ] Obtain approval for this ExecPlan before implementation.

## Surprises & Discoveries

- Observation: this branch already has template-based fast provisioning via
  `ensure_template_exists` in `test-util/src/postgres.rs`, but dependencies are
  still pinned to `v0.4.0`. Evidence: `test-util/src/postgres.rs` currently
  uses `cluster.ensure_template_exists(...)`; `Cargo.toml` and
  `test-util/Cargo.toml` pin `version = "0.4.0"`. Impact: migration can focus
  on lifecycle/send-safe upgrades plus dependency bump rather than introducing
  template cloning from scratch.

- Observation: current test helper only uses `TestCluster::new()` and
  `TestCluster::start_async()`; no split handle/guard usage exists yet.
  Evidence: symbol search only found those constructors in
  `test-util/src/postgres.rs`. Impact: adopting `new_split`/`start_async_split`
  is a concrete place to realize v0.5.0 benefits for shared/send-safe test
  contexts.

## Decision Log

- Decision: include developer-guide updates in the planning change rather than
  waiting until implementation. Rationale: the request explicitly requires
  documenting strategy/test usage, and this keeps the plan self-contained for
  the next implementer. Date/Author: 2026-02-09 / Codex.

- Decision: migration plan will explicitly adopt at least these v0.5.0
  capabilities: strict `PG_TEST_BACKEND` handling, handle/guard split APIs, and
  explicit cleanup strategy guidance. Rationale: these are the highest-value
  changes for this repository's current test harness shape and are called out
  in the migration guide. Date/Author: 2026-02-09 / Codex.
- Decision: keep this ExecPlan in `DRAFT` status and stop before code
  implementation. Rationale: the execplans workflow requires explicit user
  approval before implementation begins. Date/Author: 2026-02-09 / Codex.

## Outcomes & Retrospective

This plan establishes a concrete, repo-specific migration route from
`pg-embed-setup-unpriv` `v0.4.0` to `v0.5.0` for Postgres tests. It identifies
exact files and APIs to change, the validation gates to pass, and the
behavioural checks needed to confirm no regressions in test semantics.

Retrospective notes will be added after implementation completes.

## Context and orientation

The Postgres test setup currently lives in `test-util/src/postgres.rs` and is
consumed through test helpers in the root test suite.

Primary migration touchpoints:

- `Cargo.toml` (`[dev-dependencies]`):
  `pg-embedded-setup-unpriv = { package = "pg-embed-setup-unpriv",`
  `version = "0.4.0", features = ["diesel-support"] }`
- `test-util/Cargo.toml` (`[dependencies]`):
  `pg-embedded-setup-unpriv = { package = "pg-embed-setup-unpriv",`
  `version = "0.4.0", optional = true, features = ["async-api"] }`
- `test-util/src/postgres.rs`:
  contains embedded/external setup and teardown (`PostgresTestDb`,
  `start_embedded_postgres_with_strategy`, `start_embedded_postgres_async`).
- `tests/common.rs` and `tests/postgres_env.rs`:
  integration skip behaviour and external-db expectations.
- `docs/developers-guide.md`:
  contributor-facing instructions for running Postgres-backed tests.

Relevant v0.5.0 features from migration/user guides:

- Handle/guard split APIs (`TestCluster::new_split()`,
  `TestCluster::start_async_split()`, `ClusterHandle`, `ClusterGuard`).
- Send-safe shared fixture path (`test_support::shared_test_cluster_handle()`).
- Stricter `PG_TEST_BACKEND` contract.
- Explicit cleanup controls (`CleanupMode::{DataOnly, Full, None}`).
- Test-focused settings constructors and improved template workflow support.

## Plan of work

Stage A: Dependency and behaviour baseline (no functional refactor yet).

- Update both dependency declarations from `0.4.0` to `0.5.0`.
- Run a fast compile/test probe for Postgres paths and record any API breaks.
- Go/no-go: proceed only once baseline compile issues are understood.

Stage B: Lifecycle and fixture refactor in `test-util/src/postgres.rs`.

- Refactor embedded-cluster ownership to use v0.5.0 split APIs where beneficial
  for send-safe/shared paths.
- Keep external `POSTGRES_TEST_URL` flow unchanged in behaviour.
- Preserve or improve template clone performance path.
- Add/adjust focused tests for:
  - accepted/unsupported `PG_TEST_BACKEND` behaviour,
  - teardown semantics relevant to selected cleanup mode,
  - any new send-safe helper usage introduced.
- Go/no-go: proceed only when targeted tests pass.

Stage C: Documentation and strategy alignment.

- Update `docs/developers-guide.md` to describe the actual post-migration
  strategy and usage patterns (including when to use shared/split fixtures and
  how to handle backend selection).
- Ensure guidance is internally consistent with the code and the two pg-embed
  docs.

Stage D: Full validation and commit sequence.

- Run `make check-fmt`, `make lint`, and `make test` with `tee` logs.
- Inspect logs for hidden truncation issues.
- Commit in atomic units, each gated by the required commands.

## Concrete steps

All commands run from repository root:
`/data/leynos/Projects/mxd.worktrees/adopt-pg-embed-setup-v0-5-0`.

Use branch-aware logs for long outputs:

- `/tmp/check-fmt-$(basename "$(pwd)")-$(git branch --show).out`
- `/tmp/lint-$(basename "$(pwd)")-$(git branch --show).out`
- `/tmp/test-$(basename "$(pwd)")-$(git branch --show).out`

Planned execution sequence:

1. `make check-fmt | tee /tmp/check-fmt-$(basename "$(pwd)")-$(git branch --show).out`
2. `make lint | tee /tmp/lint-$(basename "$(pwd)")-$(git branch --show).out`
3. `make test | tee /tmp/test-$(basename "$(pwd)")-$(git branch --show).out`

Optional targeted probes during implementation:

- `cargo test -p test-util --no-default-features --features postgres`
- `cargo test --test postgres_env --no-default-features --features "postgres test-support"`

Expected success snippets:

- `Finished` lines for all cargo invocations.
- `test result: ok` for targeted tests.
- Make targets exit with status `0`.

## Validation and acceptance

Acceptance criteria:

- Dependencies:
  - `Cargo.toml` and `test-util/Cargo.toml` use `pg-embed-setup-unpriv`
    `0.5.0`.
- Behaviour:
  - embedded Postgres tests still initialize/skip as designed.
  - external Postgres mode still creates isolated per-test databases.
  - template-based fast provisioning still works and remains migration-aware.
- Documentation:
  - `docs/developers-guide.md` accurately describes the active strategy.
- Quality gates:
  - `make check-fmt` passes.
  - `make lint` passes.
  - `make test` passes.

Quality method:

- Run the concrete steps above and confirm log files contain no errors.

## Idempotence and recovery

- All migration edits are source-level and re-runnable.
- If a step fails, fix the reported issue and rerun that same command.
- If dependency/API incompatibility causes broad breakage, revert only the
  latest focused commit and retry with a narrower change.
- Keep commits small so partial rollback remains surgical.

## Artifacts and notes

Planned implementation commits (gated):

1. Dependency bump and immediate compile fixes.
2. Postgres test helper refactor for v0.5.0 features.
3. Documentation alignment (`docs/developers-guide.md`) and any related test
   assertions.

## Interfaces and dependencies

Dependencies to end with:

- Root `Cargo.toml` dev-dependency:
  `pg-embedded-setup-unpriv` (`package = "pg-embed-setup-unpriv"`,
  `version = "0.5.0"`, `features = ["diesel-support"]`).
- `test-util/Cargo.toml` dependency:
  `pg-embedded-setup-unpriv` (`package = "pg-embed-setup-unpriv"`,
  `version = "0.5.0"`, optional, `features = ["async-api"]`).

Key interfaces likely touched:

- `test_util::postgres::PostgresTestDb`
- `test_util::postgres::start_embedded_postgres_with_strategy`
- `test_util::postgres::start_embedded_postgres_async`
- `tests/common.rs` skip-on-unavailable behaviour
- `tests/postgres_env.rs` external backend behavioural expectations

## Revision note

Initial draft created after reviewing the v0.5.0 migration and user guides,
inspecting current Postgres test helper integration points, and mapping a
concrete staged migration path with explicit validation gates.

Revision (2026-02-09): marked documentation updates and required quality-gate
runs as complete in `Progress`, and recorded the explicit decision to await
approval before implementation.
