# Port wireframe regression tests to the wireframe server binary

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises and discoveries`, `Decision log`, and
`Outcomes and retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 1.6.1 requires regression coverage to exercise the wireframe
server through the `mxd-wireframe-server` binary, not only through in-process
route invocation. The immediate goal is to make `cargo test` and repository
quality gates run login, presence, file listing, and news flows against the
real binary transport path.

Success is observable when:

- behavioural and integration tests that currently call
  `process_transaction_bytes` directly are ported to binary-backed execution
  where appropriate;
- `rstest` unit coverage exists for new shared test harness logic (happy and
  unhappy paths);
- `rstest-bdd` scenarios cover happy, unhappy, and edge cases for login,
  presence-related session behaviour, file listing, and news operations against
  the running wireframe binary;
- `docs/design.md` records design decisions taken during the migration;
- `docs/users-guide.md` is updated when behaviour or user-visible execution
  changes;
- roadmap entry 1.6.1 in `docs/roadmap.md` is marked done on completion.

## Constraints

- Keep the wireframe migration dependency boundary intact: this task depends on
  roadmap 1.4 outputs and must not reintroduce legacy transport coupling.
- Preserve transport realism in integration coverage: target the
  `mxd-wireframe-server` binary via `TestServer` helpers in `test-util`.
- Keep deterministic tests. Reuse existing DB setup fixtures and avoid global
  mutable process state beyond scoped environment helpers.
- Add or update `rstest` unit tests for new harness logic and
  `rstest-bdd` behavioural tests for user-observable flows where applicable.
- Ensure local Postgres-backed validation follows
  `docs/pg-embed-setup-unpriv-users-guide.md`.
- Keep documentation in en-GB-oxendict spelling and wrap prose at 80 columns.
- Do not add new dependencies unless escalation is explicitly approved.
- Follow repository gateways before each commit:
  `make check-fmt`, `make lint`, `make test`, `make markdownlint`, and
  `make nixie` when Markdown diagrams are touched.

## Tolerances (exception triggers)

- Scope: if implementation exceeds 22 files changed or 700 net lines, stop and
  escalate with options.
- Interface: if public runtime CLI behaviour must change beyond test harness
  needs, stop and escalate.
- Presence ambiguity: if "presence flow" cannot be mapped to currently
  implemented transactions without adding net-new server features from roadmap
  2.1, stop and request direction before proceeding.
- Dependencies: if any new crate is required, stop and escalate.
- Rework loop: if quality gates fail twice after targeted fixes, stop and
  escalate with captured logs.

## Risks

- Risk: converting in-process BDD worlds to network-backed worlds introduces
  flaky socket timing. Severity: medium. Likelihood: medium. Mitigation:
  centralize handshake, timeout, and frame send/receive helpers in `test-util`;
  keep explicit timeout values.
- Risk: Postgres-backed runs become environment-sensitive.
  Severity: medium. Likelihood: medium. Mitigation: bootstrap embedded
  PostgreSQL with `pg_embedded_setup_unpriv` and preserve existing skip logic
  for unavailable backends.
- Risk: semantic drift between legacy in-process assertions and binary-backed
  assertions during migration. Severity: high. Likelihood: medium. Mitigation:
  port scenarios incrementally and keep behavioural assertions identical unless
  a documented decision changes expected output.
- Risk: roadmap acceptance references "presence" while full presence parity is
  scheduled later in roadmap 2.1. Severity: high. Likelihood: medium.
  Mitigation: explicitly document the operational definition used for 1.6.1 and
  tie it to concrete tests.

## Progress

- [x] (2026-02-10 00:00Z) Drafted ExecPlan for roadmap item 1.6.1 with
      required testing, documentation, and roadmap-close criteria.
- [x] Audit direct route-invocation test suites and classify each as keep
      unit-level vs port to binary-backed coverage.
- [x] Implement shared binary-backed BDD/world test utilities in `test-util`.
- [x] Port behavioural suites for login and routing file/news flows to binary
      execution.
- [x] Add presence-flow coverage against the binary (or document and approve
      scoped interpretation when 2.1 functionality is not yet implemented).
- [x] Add `rstest` unit tests for new harness utilities (happy/unhappy/edge).
- [x] Update `docs/design.md` and `docs/users-guide.md`.
- [x] Mark `docs/roadmap.md` item 1.6.1 as done with completion note.
- [x] Run full quality gates with tee-captured logs and commit atomic changes.

## Surprises and discoveries

- Current behavioural suites in `tests/wireframe_routing_bdd.rs`,
  `tests/wireframe_login_compat.rs`, `tests/wireframe_xor_compat.rs`, and
  `tests/session_privileges_bdd.rs` use in-process route execution through
  `process_transaction_bytes` via `test-util/src/wireframe_bdd_world.rs`.
- Existing integration suites such as `tests/file_list.rs`,
  `tests/news_categories.rs`, and `tests/news_articles.rs` already start the
  `mxd-wireframe-server` binary through `tests/common.rs` and `TestServer`.
- The referenced Postgres setup guide path in the request uses
  `pg-embedded-setup-unpriv`, while the repository document path is
  `docs/pg-embed-setup-unpriv-users-guide.md`.
- Server-only routing scenarios (unknown transaction type) still need an
  active TCP connection and therefore must start `TestServer` with a no-op DB
  setup; otherwise the binary-backed world has no stream to send on.
- XOR compatibility state cannot be observed directly from the binary-backed
  harness. A stable probe is to attempt a plaintext login: success implies XOR
  compatibility is disabled, while failure after prior XOR traffic implies it
  is enabled.

## Decision log

- Decision: treat binary-backed transport tests as the canonical regression
  path for roadmap 1.6.1, while retaining focused pure unit tests for local
  logic where transport is not the subject. Rationale: aligns acceptance with
  "server binary under test" while preserving fast, deterministic unit
  coverage. Date/Author: 2026-02-10 / Codex (ExecPlan draft).
- Decision: defer any net-new presence feature implementation outside 1.4 scope
  unless required to satisfy agreed 1.6.1 presence coverage semantics.
  Rationale: roadmap phase ordering places full presence parity in 2.1.
  Date/Author: 2026-02-10 / Codex (ExecPlan draft).
- Decision: define 1.6.1 "presence" as authenticated-session continuity on a
  single connection (login followed by a privileged request that no longer
  returns `ERR_NOT_AUTHENTICATED`), while keeping full user-list presence for
  roadmap 2.1. Rationale: current implementation scope includes session gating
  but not the user-list/presence transaction family. Date/Author: 2026-02-10 /
  Codex.
- Decision: keep truncated-frame behavioural expectations in unit/parser
  coverage rather than binary BDD scenarios. Rationale: partial socket writes
  do not constitute complete wireframe frames, so immediate error replies are
  not a stable transport contract. Date/Author: 2026-02-10 / Codex.

## Outcomes and retrospective

Intended outcomes once implemented:

- Wireframe regression tests validate routing and compatibility through the
  actual binary runtime path.
- Test harness duplication between in-process worlds and integration helpers is
  reduced by shared binary-backed utilities.
- Documentation and roadmap status stay in sync with delivered behaviour.

Retrospective placeholder:

- Implemented:
  `test-util/src/wireframe_bdd_world.rs` now launches `mxd-wireframe-server`
  and exchanges framed transactions over TCP.
- Implemented:
  `tests/wireframe_routing_bdd.rs`, `tests/wireframe_login_compat.rs`, and
  `tests/wireframe_xor_compat.rs` now run against the binary with existing
  feature scenarios updated for transport-realistic expectations.
- Implemented:
  `test-util/src/protocol.rs` now supports handshake sub-version overrides and
  adds `rstest` coverage for handshake happy/unhappy paths.
- Implemented:
  `docs/design.md`, `docs/users-guide.md`, and `docs/roadmap.md` record the
  migration decisions and mark roadmap item 1.6.1 done.
- Did not implement:
  full user-list presence transactions or runtime privilege persistence (both
  remain planned in roadmap 2.1+ and separate tasks).
- Lesson:
  binary-backed BDD coverage should focus on complete wire-level transactions;
  parser truncation edge cases remain better validated at unit-test level.

## Context and orientation

Primary files and modules in current state:

- `docs/roadmap.md`: source of task 1.6.1 acceptance and dependency.
- `test-util/src/server/mod.rs`: binary launcher (`mxd-wireframe-server`) and
  readiness logic.
- `tests/common.rs`: integration entry point that starts `TestServer`.
- `tests/file_list.rs`, `tests/news_categories.rs`, `tests/news_articles.rs`:
  binary-backed integration flows already in place.
- `test-util/src/wireframe_bdd_world.rs`: shared binary-backed BDD world that
  now starts `TestServer`, handshakes over TCP, and exchanges framed
  transactions.
- `tests/wireframe_routing_bdd.rs` and
  `tests/features/wireframe_routing.feature`: route-level BDD scenarios now
  execute against `mxd-wireframe-server`.
- `tests/wireframe_login_compat.rs` and
  `tests/features/wireframe_login_compat.feature`: login compatibility BDD
  scenarios now execute against `mxd-wireframe-server`.
- `tests/session_privileges_bdd.rs` and
  `tests/features/session_privileges.feature`: auth/privilege behavioural
  scenarios remain in-process to preserve explicit unprivileged-session branch
  coverage until privilege persistence is modelled in runtime state.
- `docs/design.md`: design record to update with migration decisions.
- `docs/users-guide.md`: user-facing operational behaviour guide to update if
  runtime behaviour or testing expectations become user-relevant.

## Plan of work

### Stage A: baseline audit and scope lock

Catalogue all tests that exercise wireframe routing semantics and classify them
into:

- tests that must run against the binary (integration and behaviour coverage);
- tests that should remain pure unit tests (codec/parser/helper logic).

Document the selected definition of "presence flow" for 1.6.1 and map it to
existing implemented transactions. If this mapping is ambiguous, stop and
escalate per tolerances.

Validation gate for Stage A:

- A short mapping table is added to the Decision log and used to drive Stage B.

### Stage B: introduce binary-backed behavioural world utilities

Create or refactor test support utilities so BDD suites can:

- start `mxd-wireframe-server` through `TestServer`;
- establish TCP connection, perform handshake, optionally login;
- send framed transactions and decode replies;
- reuse DB fixture setup (`setup_files_db`, `setup_news_db`, etc.);
- preserve backend skip behaviour for unavailable Postgres environments.

Prefer extending `test-util` helpers instead of duplicating transport logic in
multiple test files.

Add `rstest` unit tests for new utility behaviour, including:

- successful startup + handshake path;
- graceful error propagation when framing or handshake fails;
- edge handling for skipped backend environments.

Validation gate for Stage B:

- `cargo test -p test-util` passes for the active feature set.

### Stage C: port BDD suites to binary execution

Port high-value behavioural suites from in-process routing to binary-backed
execution:

- login compatibility scenarios (`wireframe_login_compat`);
- routing scenarios covering login, file list, and news list flows
  (`wireframe_routing_bdd`);
- presence-related behavioural scenarios according to Stage A mapping
  (expected to include authenticated session behaviour and privilege-gated
  operations currently implemented).

Keep scenario language in `.feature` files stable unless behaviour definitions
need correction.

Ensure unhappy and edge paths remain covered, including malformed frames,
unknown transaction types, invalid payloads, and compatibility gating paths.

Validation gate for Stage C:

- targeted `cargo test` runs for ported suites pass under sqlite and postgres
  feature sets.

### Stage D: integrate docs and roadmap closure

Update `docs/design.md` with implementation decisions taken during this
migration, including:

- why binary-backed BDD was chosen over direct route invocation;
- how presence coverage was defined for 1.6.1;
- any trade-offs in test speed vs fidelity.

Update `docs/users-guide.md` if user-visible server behaviour changed, or add a
brief clarification if only validation posture changed but operational
behaviour did not.

After all acceptance criteria are validated, mark roadmap item 1.6.1 as done in
`docs/roadmap.md` with completion date and concise summary.

Validation gate for Stage D:

- documentation lints and formatting checks pass.

### Stage E: final quality gates and atomic commits

Run all required gates with tee logs, review logs for failures, and commit in
small logical units. Do not commit until all relevant gates pass.

Recommended atomic commit split:

- Commit 1: test-util harness additions + unit tests.
- Commit 2: BDD/integration test migrations for binary-backed execution.
- Commit 3: documentation updates (`design`, `users-guide`, `roadmap`).

## Concrete steps

1. Capture audit baseline and decision notes.

   Commands: `git status --short`
   `grepai search "process_transaction_bytes RouteContext" --json --compact`
   `grepai search "TestServer::start_with_setup wireframe" --json --compact`

2. Implement binary-backed world helpers in `test-util`.

   Commands:

   ```sh
   cargo test -p test-util 2>&1 | tee \
     /tmp/test-test-util-$(basename "$PWD")-$(git branch --show).out
   ```

3. Port BDD suites and presence coverage.

   Commands:

   ```sh
   cargo test --test wireframe_routing_bdd --test wireframe_login_compat \
     --test session_privileges_bdd 2>&1 | tee \
     /tmp/test-wireframe-bdd-$(basename "$PWD")-$(git branch --show).out
   ```

4. Validate existing integration flows still pass against binary.

   Commands:

   ```sh
   cargo test --test file_list --test news_categories --test news_articles \
     2>&1 | tee \
     /tmp/test-wireframe-integration-$(basename "$PWD")-$(git branch --show).out
   ```

5. Prepare Postgres local environment for full gates.

   Commands: `cargo install --locked pg-embed-setup-unpriv`
   `pg_embedded_setup_unpriv`

6. Run repository quality gates.

   Commands:

   ```sh
   make check-fmt 2>&1 | tee \
     /tmp/check-fmt-$(basename "$PWD")-$(git branch --show).out
   make lint 2>&1 | tee \
     /tmp/lint-$(basename "$PWD")-$(git branch --show).out
   make test 2>&1 | tee \
     /tmp/test-$(basename "$PWD")-$(git branch --show).out
   make markdownlint 2>&1 | tee \
     /tmp/markdownlint-$(basename "$PWD")-$(git branch --show).out
   make nixie 2>&1 | tee \
     /tmp/nixie-$(basename "$PWD")-$(git branch --show).out
   make fmt 2>&1 | tee \
     /tmp/fmt-$(basename "$PWD")-$(git branch --show).out
   ```

7. Apply documentation updates and roadmap closure.

   Files: `docs/design.md` `docs/users-guide.md` `docs/roadmap.md`

8. Commit each atomic change after gates pass.

   Commands: `git add <paths>` `git commit`

## Validation and acceptance

Behavioural acceptance for roadmap 1.6.1:

- `cargo test` must exercise login, presence-defined flows, file listing, and
  news flows against `mxd-wireframe-server` started by the test harness.
- Ported `rstest-bdd` scenarios pass for happy, unhappy, and edge cases.
- New `rstest` unit tests for harness utilities pass.
- Existing binary-backed integration suites (`file_list`, `news_categories`,
  `news_articles`) remain green.

Quality acceptance:

- `make check-fmt` passes.
- `make lint` passes.
- `make test` passes.
- `make markdownlint` passes.
- `make nixie` passes for changed Markdown files with Mermaid.
- `docs/roadmap.md` item 1.6.1 is marked done only after all above pass.

## Idempotence and recovery

- Test harness setup must remain re-runnable. `TestServer` uses ephemeral ports
  and temporary DB state, so reruns should not require manual cleanup.
- Embedded Postgres setup via `pg_embedded_setup_unpriv` is idempotent and can
  be rerun if bootstrap state is stale.
- If a stage fails, revert only the partial stage changes, keep prior commits,
  and resume from the last passing stage gate.

## Artifacts and notes

Keep the following artefacts during implementation:

- gate logs in `/tmp/*-$(basename "$PWD")-$(git branch --show).out`;
- failing scenario/test names copied into the Decision log before fixes;
- final acceptance command transcript summaries captured in commit messages or
  follow-up notes.

## Interfaces and dependencies

Core interfaces expected to be used and preserved:

- `test_util::TestServer` for binary process lifecycle.
- `test_util::handshake` and `test_util::login` for protocol setup.
- `test_util` DB fixture setup functions for deterministic data.
- wireframe transaction framing types in `mxd::transaction::*` for request and
  response encoding.

No new dependencies are expected for this task.

## Revision note

- 2026-02-10: Initial draft created from roadmap 1.6.1 requirements and
  current wireframe test harness state.
- 2026-02-10: Implementation in progress. Binary-backed world refactor and BDD
  migrations landed locally; documentation and roadmap entries updated pending
  final quality-gate run and commit.
- 2026-02-10: Final quality gates passed (`make check-fmt`, `make lint`,
  `make test-postgres`, `make test-sqlite`, `make test-wireframe-only`,
  `make test-verification`, `make markdownlint`, `make nixie`); plan marked
  complete.
