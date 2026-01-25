# Adopt rstest-bdd v0.4.0 with async scenarios

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

No PLANS.md was found in the repository root.

## Purpose / Big Picture

Upgrade the test suite to rstest-bdd v0.4.0 so behaviour tests can run in
Tokio's current-thread async runtime and benefit from improved fixture lint
safety. Success means BDD tests still pass under `cargo test`, at least one BDD
scenario exercises the async scenario path described in
`docs/rstest-bdd-users-guide.md`, and the full quality gate (`make check-fmt`,
`make lint`, `make test`) passes.

## Constraints

- Keep Rust edition 2024 and rust-version 1.85 unchanged.
- Do not add new external dependencies beyond the rstest-bdd version bump
  unless the upgrade requires them.
- Preserve existing behaviour coverage: every current BDD scenario must still
  run and assert the same outcomes.
- Keep step definitions synchronous; async work must follow the guidance in
  `docs/rstest-bdd-users-guide.md`.
- Use Makefile targets for validation and capture long outputs with `tee`.

## Tolerances (Exception Triggers)

- Scope: more than 20 files modified or more than 800 net LOC changed.
- Interface: any public API signature changes outside test code.
- Dependencies: any new crate added beyond rstest-bdd and rstest-bdd-macros.
- Validation: if `make lint` or `make test` fails twice for the same issue.
- Ambiguity: if async conversion requires a design choice that changes test
  semantics (e.g., step ordering or fixture lifetimes).

## Risks

    - Risk: v0.4.0 macro changes could introduce compile-time validation
      breakage or warnings in existing step definitions.
      Severity: medium
      Likelihood: medium
      Mitigation: upgrade in a focused commit, run `make check-fmt` and a
      targeted test subset first, then the full suite.

    - Risk: Async scenario support requires different patterns for async work;
      existing steps call `Runtime::block_on`, which may not be compatible with
      async scenarios.
      Severity: medium
      Likelihood: high
      Mitigation: only convert scenarios where async work can move into async
      fixtures or the async test body; otherwise leave steps synchronous.

    - Risk: Fixture lint safety improvements may change how unused fixtures are
      treated, leading to new or removed warnings.
      Severity: low
      Likelihood: medium
      Mitigation: remove manual `let _ = world;` only after confirming the new
      macro suppresses unused variable warnings for the affected cases.

## Progress

    - [x] (2026-01-25 00:00Z) Read `docs/rstest-bdd-users-guide.md` async and
      fixture lint safety guidance.
    - [x] (2026-01-25 00:00Z) Inventory BDD tests and identify worlds using
      `tokio::runtime::Runtime` via `leta` and `rg`.
    - [x] (2026-01-25 00:10Z) Begin implementation and record plan updates.
    - [x] (2026-01-25 00:14Z) Update rstest-bdd dependencies to v0.4.0.
    - [x] (2026-01-25 00:16Z) Select runtime-selection scenario for async
      execution and convert it to Tokio current-thread.
    - [x] (2026-01-25 00:18Z) Adopt fixture lint safety improvements by
      switching create-user BDD tests to `scenarios!` with tags.
    - [x] (2026-01-25 01:05Z) Run `make fmt`, `make check-fmt`,
      `make markdownlint`, `make nixie`, `make lint`, and `make test`.
    - [ ] Commit in atomic steps.

## Surprises & Discoveries

    - Observation: No `scenarios!` macro usage exists in `tests/`.
      Evidence: `rg "scenarios!" tests` returned no matches.
      Impact: fixture lint safety improvements may require either adopting
      `scenarios!` or confirming `#[scenario]` now suppresses unused fixtures.
    - Observation: Formatting tools updated documentation tables and the
      `.gitignore` comment describing the grepai index.
      Evidence: `make fmt` changed `docs/pg-embed-setup-unpriv-users-guide.md`,
      `docs/rstest-bdd-users-guide.md`, and `.gitignore`.
      Impact: include these in the documentation commit to keep the tree clean.

## Decision Log

    - Decision: Keep step functions synchronous and prefer async fixtures or
      async scenario bodies when adopting async execution.
      Rationale: The v0.4.0 guidance states steps remain synchronous and warns
      against nested runtimes inside async scenarios.
      Date/Author: 2026-01-25 / Codex
    - Decision: Use `scenarios!` with a feature tag to generate create-user
      tests and rely on the macro's unused-fixture suppression.
      Rationale: Demonstrates the fixture lint safety improvements without
      duplicating existing scenario tests.
      Date/Author: 2026-01-25 / Codex
    - Decision: Convert `tests/runtime_selection_bdd.rs` scenarios to async
      Tokio current-thread tests to exercise async scenario support.
      Rationale: The scenarios are synchronous and safe to execute under the
      async runner without nested runtimes.
      Date/Author: 2026-01-25 / Codex

## Outcomes & Retrospective

Upgraded rstest-bdd dependencies to v0.4.0, adopted async scenarios in
`tests/runtime_selection_bdd.rs`, and switched the create-user BDD suite to
`scenarios!` with fixtures and tags to exercise fixture lint safety. All
quality gates passed (`make check-fmt`, `make lint`, `make test`, plus
documentation formatting and diagram validation). Future work could extend
async scenarios to additional BDD suites that can move async work into fixtures.

## Context and Orientation

The repository uses rstest-bdd for BDD tests under `tests/` and feature files
under `tests/features/`. The dev dependencies in `Cargo.toml` now pin
`rstest-bdd = "0.4.0"` and `rstest-bdd-macros` (version `0.4.0` with the
`compile-time-validation` feature). Several BDD worlds embed a
`tokio::runtime::Runtime` and call `block_on` inside step functions:

- `tests/create_user_bdd.rs` (`CreateUserWorld`)
- `tests/outbound_messaging_bdd.rs` (`OutboundWorld`)
- `tests/session_privileges_bdd.rs` (`PrivilegeWorld`)
- `tests/wireframe_routing_bdd.rs` (`RoutingWorld`)

The users guide documents v0.4.0 async scenario execution and fixture lint
safety. Async scenarios require `#[tokio::test(flavor = "current_thread")]` and
`async fn` scenario bodies; steps remain synchronous. The guide notes that the
`scenarios!` macro adds `#[expect(unused_variables)]` when fixtures are
present, addressing unused fixture warnings. The create-user BDD suite now uses
`scenarios!` with a feature tag to rely on this behaviour.

## Plan of Work

Stage A: Confirm upgrade expectations and identify candidates.

1. Review `docs/rstest-bdd-users-guide.md` sections on async scenarios and
   fixture lint safety to capture required patterns and limitations.
2. Map current BDD tests to their feature files and note where async work
   happens (e.g., `block_on` inside steps). Decide which scenarios can migrate
   without altering behaviour.
3. Check for any local documentation in `docs/` or `tests/` that references
   rstest-bdd versions or patterns that will change.

Stage B: Dependency bump and compilation probe.

1. Update `Cargo.toml` dev dependencies to `rstest-bdd = "0.4.0"` and
   `rstest-bdd-macros` version `0.4.0` with the `compile-time-validation`
   feature enabled. Keep caret requirements intact.
2. Run `make check-fmt` and a targeted build/test for a single BDD test target
   (for example, `cargo test --test create_user_bdd`) to validate the upgrade
   before wider changes.

Stage C: Async adoption and fixture lint safety.

1. Choose at least one BDD scenario whose async operations can be performed in
   async fixtures or the async test body. Convert it to:
   - `#[tokio::test(flavor = "current_thread")]`
   - `async fn` scenario body
   - Steps that stay synchronous and operate on already-resolved data.
2. For remaining BDD tests that still use `Runtime::block_on`, keep them
   synchronous unless behaviour can be preserved under async scenarios.
3. Re-evaluate unused fixture suppression:
   - If v0.4.0 `#[scenario]` now suppresses unused fixture warnings, remove
     `let _ = world;` statements.
   - Otherwise, retain the existing suppressions or adopt `scenarios!` where
     that reduces boilerplate without changing behaviour.

Stage D: Documentation and cleanup.

1. Update any internal docs that reference rstest-bdd usage patterns or the
   previous version. Mention async scenarios and fixture lint behaviour where
   relevant.
2. Run the full quality gates (`make check-fmt`, `make lint`, `make test`) and
   capture logs via `tee`.
3. Commit the dependency bump and test updates as separate, gated commits.

## Concrete Steps

All commands run from the repository root
(`/data/leynos/Projects/mxd.worktrees/adopt-rstest-bdd-v0-4-0`). For long
outputs, capture logs using:

    /tmp/$ACTION-$(get-project)-$(git branch --show).out

If `get-project` is unavailable, replace it with `$(basename "$(pwd)")`.

1. Update dependencies in `Cargo.toml` (dev-dependencies).
2. Probe build/test on one BDD target:

    cargo test --test create_user_bdd

3. Run formatting and lint gates:

    make check-fmt | tee /tmp/check-fmt-$(get-project)-$(git branch --show).out
    make lint | tee /tmp/lint-$(get-project)-$(git branch --show).out

4. Run the full test suite:

    make test | tee /tmp/test-$(get-project)-$(git branch --show).out

## Validation and Acceptance

Acceptance criteria:

- `Cargo.toml` uses rstest-bdd v0.4.0 for both runtime and macros.
- At least one BDD scenario executes under Tokio current-thread async runtime
  and continues to assert the same outcomes.
- No new lint warnings about unused fixtures after any lint-suppression
  adjustments.
- `make check-fmt`, `make lint`, and `make test` all pass.

Quality method:

- Run the commands listed in `Concrete Steps` and review the `tee` logs for
  errors or warnings.

## Idempotence and Recovery

The dependency bump and test conversions are reversible by editing `Cargo.toml`
and reverting the specific test files. If a step fails, rerun the command after
addressing the reported error. Keep intermediate edits small to allow surgical
rollbacks.

## Artifacts and Notes

Expected command snippets (abbreviated):

    Finished test [unoptimized + debuginfo] target(s) in …
    Running tests/create_user_bdd.rs
    test … ok

## Interfaces and Dependencies

Dependencies to update:

    rstest-bdd = "0.4.0"
    rstest-bdd-macros = { version = "0.4.0", features = ["compile-time-validation"] }

Key interfaces:

- `#[scenario]` test functions in `tests/*_bdd.rs`
- Optional adoption of `scenarios!` macro for fixture lint suppression if
  needed.
- Tokio current-thread runtime via `#[tokio::test(flavor = "current_thread")]`
  on async scenario functions.

## Revision note

Initial draft created after reviewing the user guide and existing BDD tests.
Updated to reflect dependency bumps and selected async/fixture-lint adoption.
