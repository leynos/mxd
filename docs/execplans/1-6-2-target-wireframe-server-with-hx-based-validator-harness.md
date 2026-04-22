# Extend the hx validator harness to target the wireframe server

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: IN PROGRESS

## Purpose / big picture

Roadmap item 1.6.2 requires the `hx`-driven validator harness to become an
authoritative regression path for the wireframe runtime, not a best-effort
compatibility smoke test. The immediate goal is to make the validator crate
target `mxd-wireframe-server` explicitly, cover the required end-to-end flows,
and run that coverage in CI instead of silently skipping when the environment
is not prepared.

Success is observable when:

- validator tests target `mxd-wireframe-server` explicitly rather than relying
  on implicit defaults in shared helpers;
- the harness has clear flow coverage for login, file download, chat, and news
  against the wireframe server, with happy and unhappy paths plus relevant edge
  cases;
- new harness helpers are covered with `rstest` unit tests, and
  `rstest-bdd` scenarios are added where they improve readability for
  higher-level validation behaviour;
- devboxer and continuous integration (CI) both provision SynHX through a
  shared `scripts/install-synhx.sh` script and fail closed when validator
  coverage cannot run;
- `docs/design.md` records the final harness-targeting and CI decisions;
- `docs/users-guide.md` is updated if the implementation changes user-visible
  runtime behaviour or supported validation workflows;
- roadmap item 1.6.2 in `docs/roadmap.md` is marked done only after the above
  pass.

## Constraints

- Keep the validator harness focused on the wireframe runtime. This task must
  not reintroduce ambiguity between `mxd` and `mxd-wireframe-server`.
- Preserve real-client validation via `hx` plus `expectrl`; do not replace the
  validator with an in-process protocol shim.
- Extract reusable harness logic into unit-testable helpers and cover that
  logic with `rstest`.
- Add `rstest-bdd` behavioural coverage where it clarifies multi-step harness
  orchestration or validation contracts; do not force BDD onto low-level
  pseudo-terminal (PTY) details that are better expressed as ordinary tests.
- Use dependency injection for process-launch, filesystem, and environment
  seams where that materially improves deterministic testing, following
  `docs/reliable-testing-in-rust-via-dependency-injection.md`.
- Use `pg-embed-setup-unpriv` for documented local Postgres validator runs
  rather than bespoke cluster setup.
- Use a repository-owned SynHX install script rather than inline CI shell
  fragments, so devboxer and CI share the same provisioning path.
- Keep documentation in en-GB-oxendict spelling and wrap prose at 80 columns.
- Do not add a new third-party dependency unless escalation is explicitly
  approved. Reusing workspace crates such as `rstest-bdd` is allowed.
- Before any implementation commit, run the repository quality gates plus any
  new validator-specific gates introduced by this work: `make check-fmt`,
  `make lint`, `make test`, `make markdownlint`, `make nixie`, and explicit
  validator commands if they are not folded into `make test`.

## Tolerances (exception triggers)

- Scope: if completing 1.6.2 requires implementing substantial new server
  protocol features beyond harness and test infrastructure, stop and review the
  dependency mismatch before proceeding.
- Acceptance mismatch: if file download or chat validation requires roadmap
  tasks that are still incomplete in phases 2.x or 3.x, stop and either obtain
  approval for scoped substitute coverage or re-sequence the dependency chain.
- Client capability: if SynHX `hx` v0.1.48.1 cannot script one of the required
  flows in headless mode, stop after confirming the exact limitation from `hx`
  help or source and present options.
- CI stability: if the `hx` install/provision path remains flaky after two
  focused fixes, stop and review whether the binary should be cached, pinned,
  or supplied differently.
- Validation: if validator tests require serial execution or a non-`nextest`
  runner, document that explicitly and do not silently wedge them into an
  incompatible existing target.

## Risks

- Risk: roadmap item 1.6.2 names file download and chat flows, but the current
  wireframe routing surface only covers login, file listing, and news.
  Severity: high. Likelihood: high. Mitigation: make the dependency mismatch an
  explicit Stage A deliverable and do not mark the task done until it is
  resolved.

- Risk: CI currently does not provision `hx`, and `hx` on developer machines
  may resolve to the Helix editor rather than SynHX. Severity: high.
  Likelihood: high. Mitigation: pin installation/version checks in CI and keep
  the Helix-detection guard in reusable harness code.

- Risk: file-download validation may require secondary data-port handling,
  temporary filesystem assertions, and stricter cleanup than the current
  validator tests use. Severity: medium. Likelihood: medium. Mitigation:
  centralize temp-directory and artefact assertions in shared helpers before
  adding the end-to-end cases.

- Risk: chat validation may require multiple concurrent `hx` sessions and
  careful prompt coordination, increasing flakiness. Severity: medium.
  Likelihood: medium. Mitigation: model multi-client orchestration in one
  helper abstraction, keep explicit timeouts, and add focused unhappy-path
  assertions.

- Risk: the validator crate sits outside the workspace default members, so
  existing `make test`/CI jobs do not exercise it. Severity: medium.
  Likelihood: high. Mitigation: add explicit Makefile and CI coverage rather
  than assuming workspace defaults.

## Progress

- [x] (2026-04-10 00:00Z) Drafted ExecPlan for roadmap item 1.6.2 with current
      validator, CI, and routing constraints captured.
- [x] (2026-04-12 12:28Z) Audited the requested validator flows against the
      currently implemented wireframe transaction surface and recorded the
      remaining protocol mismatches for file listing, file download, chat, and
      news.
- [x] (2026-04-12 00:00Z) Added `scripts/install-synhx.sh` as the shared
      SynHX provisioning path for devboxer and CI.
- [x] (2026-04-12 00:00Z) Added config-gated placeholder validators for the
      pending `chat` and `file_download` flows, plus developer documentation
      describing how branches can opt into them via file or environment.
- [x] (2026-04-12 00:00Z) Added `validator/validator.toml`,
      `validator/src/config.rs`, and `validator/tests/pending_validators.rs`
      so pending validators default to disabled on `main`, can be enabled per
      branch, and fail explicitly when enabled before the server flow exists.
- [x] (2026-04-12 12:28Z) Refactored validator support code into reusable,
      unit-testable helpers under `validator/src/` for run policy, server
      binary resolution, SynHX detection/spawn, and harness orchestration.
- [x] (2026-04-12 12:28Z) Added `rstest` unit coverage for the new support
      modules. No `rstest-bdd` scenarios were added because the remaining
      unsupported flows are blocked by protocol overlap rather than
      orchestration readability.
- [x] (2026-04-12 12:28Z) Extended validator end-to-end coverage for the
      currently supported SynHX-to-wireframe overlap: successful login, failed
      authentication gating, and XOR login against `mxd-wireframe-server`.
- [x] (2026-04-12 12:28Z) Added explicit CI execution for the sqlite validator
      harness with pinned `hx` provisioning and fail-closed policy.
- [x] (2026-04-12 12:28Z) Documented local Postgres validator execution via
      `make test-validator-postgres`, which builds the Postgres wireframe
      binary through the same Makefile path used for other local runs.
- [x] (2026-04-12 12:28Z) Updated `docs/design.md` and
      `docs/developers-guide.md`, and reviewed `docs/users-guide.md` with no
      user-visible workflow change requiring documentation updates.
- [x] (2026-04-20 00:00Z) Addressed review follow-up by reusing the shared
      validator skip helper, marking exported validator error enums as
      non-exhaustive, hardening `scripts/install-synhx.sh` for unsupported
      platforms and missing `sudo`, and normalizing `pg-embed-setup-unpriv`
      command and guide references in the touched documentation.
- [ ] Keep roadmap item 1.6.2 open until real-client file/news/chat coverage
      exists, or the acceptance criteria are re-scoped.
- [ ] Run full quality gates with tee-captured logs and commit the final
      implementation.

## Surprises & Discoveries

- Observation: the existing validator crate already launches `TestServer`,
  which defaults to `mxd-wireframe-server` in `test-util/src/server/mod.rs`.
  Impact: "target the wireframe server" is partially true today, but only by
  convention and without explicit validator-level assertions.

- Observation: validator coverage remains narrower than the roadmap target.
  Evidence: the validator suite now covers successful login, failed-auth
  gating, XOR login, and config-gated placeholders for `chat` and
  `file_download`, but still lacks real-client coverage for file listing, file
  download, chat, and news. Impact: implementation progress is real, but
  roadmap acceptance is still blocked by unsupported flows.

- Observation: the workspace default members exclude `validator`.
  Evidence: `Cargo.toml` declares `default-members = ["."]`. Impact:
  `make test` and the current CI matrix do not automatically run validator
  tests.

- Observation: CI does not install `hx`.
  Evidence: `.github/workflows/ci.yml` provisions Rust, Postgres/SQLite
  dependencies, `cargo-nextest`, and linters, but nothing for SynHX. Impact:
  validator tests currently rely on skip behaviour rather than CI enforcement.

- Observation: the currently documented SynHX version in legacy design text is
  stale for this task. Evidence: the supplied installation process pins
  `HX_VERSION=0.1.48.1` from `leynos/synhx-client` releases, whereas older
  design prose mentions `0.2.4`. Impact: the plan should standardize on a
  repository-owned install script and the newer pinned version.

- Observation: pending validators need to coexist with `main` while parallel
  feature work is still incomplete. Evidence:
  `validator/tests/pending_validators.rs` now carries opt-in checks for `chat`
  and `file_download`, and `validator/validator.toml` keeps both disabled by
  default. Impact: feature branches can enable only the validators they are
  ready to satisfy without destabilizing unrelated work.

- Observation: the original `validator/tests/login.rs` suite failed in this
  environment before reaching `hx`. Evidence: earlier runs of
  `cargo test -p validator --features sqlite --test login` failed with
  `server failed protocol readiness check` from `TestServer` startup. Impact:
  the implementation needed a stable explicit server-launch path before the
  pre-existing login validator could become reliable.

- Observation: the login readiness failure was caused by implicit
  `cargo run` server startup rather than a protocol defect. Evidence: wiring
  the validator harness to a prebuilt `mxd-wireframe-server` binary resolved
  the readiness failure and made the login and XOR validators pass
  consistently. Impact: explicit binary resolution is required for stable
  validator execution in CI and local runs.

- Observation: root-container Postgres validation requires the external
  `pg_worker` helper and still does not make the validator's Postgres suite
  pass in this environment. Evidence: `make test` passed once
  `PG_EMBEDDED_WORKER` pointed at an installed `pg_worker` binary, but
  `make test-validator-postgres` still failed with
  `PostgreSQL server unreachable` from `validator/tests/login.rs`. Impact: the
  documented local Postgres validator path exists, but it is not yet a
  validated gate in this root container.

- Observation: current wireframe routes do not include chat or file-download
  transactions. Evidence: `src/wireframe/route_ids.rs` exposes only routes
  `107`, `200`, `370`, `371`, `400`, and `410`. Impact: roadmap acceptance for
  chat and file download is ahead of the currently implemented transaction
  surface.

- Observation: the current harness already detects the common "wrong hx"
  failure mode. Evidence: `validator/tests/xor_compat.rs` rejects the Helix
  editor by checking `hx --version`. Impact: that logic should be extracted and
  reused, not duplicated.

- Observation: SynHX file-list requests and replies only partially overlap with
  the current wireframe server behaviour. Evidence: SynHX sends `/ls` as
  transaction `200` with a binary `DATA_DIR` payload, which the server can now
  tolerate, but the SynHX client still expects a legacy file-list reply shape
  rather than the server's current parameter-encoded reply. Impact: file-list
  and file-download validators remain blocked even after request-side
  compatibility work.

- Observation: SynHX news commands target legacy transaction identifiers that
  do not match the current server implementation. Evidence: pinned SynHX `hx`
  uses legacy news transactions `0x65` and `0x67`, while the wireframe server
  implements `370`, `371`, `400`, and `410`. Impact: real-client news
  validation cannot be completed through SynHX without either compatibility
  shims or a different client path.

- Observation: the currently supportable always-on validator surface is smaller
  than the roadmap text suggests. Evidence: stable end-to-end coverage exists
  today for login, failed-auth gating, and XOR login only; chat and file
  download remain placeholder validators, and news/file-list coverage is
  blocked by client-server protocol mismatches. Impact: the roadmap item must
  stay open until either the server or the client compatibility surface expands.

## Decision Log

- Decision: make server targeting explicit inside the validator harness even
  though `TestServer` already defaults to `mxd-wireframe-server`. Rationale:
  roadmap acceptance is about deliberate wireframe validation, not an implicit
  default that could regress later. Date/Author: 2026-04-10 / Assistant.

- Decision: treat the current chat/file-download gap as a first-class planning
  blocker, not something to paper over with partial "done" wording. Rationale:
  roadmap phases 2.2, 2.3, and 3.x still own those transactions, so 1.6.2 needs
  an explicit sequencing decision before implementation claims completion.
  Date/Author: 2026-04-10 / Assistant.

- Decision: extract validator primitives into library modules under
  `validator/src/` and cover them with `rstest`, rather than growing ad hoc
  test-local helpers in each file. Rationale: PTY orchestration, binary
  discovery, temp downloads, and skip/fail policy are all logic worth testing
  directly. Date/Author: 2026-04-10 / Assistant.

- Decision: introduce dedicated validator Makefile/CI entrypoints instead of
  relying on workspace defaults. Rationale: `validator` is not a default member
  and its runtime requirements differ from ordinary `cargo nextest` suites.
  Date/Author: 2026-04-10 / Assistant.

- Decision: provision SynHX via `scripts/install-synhx.sh`, defaulting to
  `HX_VERSION=0.1.48.1`, with environment overrides for install directories.
  Rationale: one script gives devboxer and CI the same installation behaviour
  and removes fragile duplicated shell snippets. Date/Author: 2026-04-12 /
  Assistant.

- Decision: keep pending validators default-disabled in
  `validator/validator.toml` and allow environment variables to override the
  file on a per-run basis. Rationale: `main` should stay green while parallel
  branches opt into the validators they need, and environment overrides are the
  least-friction path for CI jobs and ad hoc branch validation. Date/Author:
  2026-04-12 / Assistant.

- Decision: enabled pending validators fail explicitly with an
  "enabled but not implemented yet" error until the underlying flow lands.
  Rationale: silent skips would hide missing coverage on branches that claim to
  implement chat or file-download support. Date/Author: 2026-04-12 / Assistant.

- Decision: document local Postgres validation via `pg-embed-setup-unpriv`,
  but do not assume the first CI cut must run both backends unless explicitly
  required during implementation. Rationale: the task acceptance says the
  harness runs in CI; the request separately asks for local Postgres
  enablement. Date/Author: 2026-04-10 / Assistant.

- Decision: resolve `mxd-wireframe-server` explicitly from a prebuilt binary in
  the validator harness before falling back to inherited cargo test binary
  hints. Rationale: compile-time startup delays from implicit `cargo run`
  exceeded the validator readiness window and produced misleading protocol
  startup failures. Date/Author: 2026-04-12 / Assistant.

- Decision: make the first CI validator job sqlite-only while still documenting
  and supporting local Postgres validator runs. Rationale: the immediate goal
  is a fail-closed real-client validator path in CI, and sqlite reaches that
  goal without requiring a second backend job before the protocol overlap is
  sufficient to justify more matrix expansion. Date/Author: 2026-04-12 /
  Assistant.

- Decision: narrow always-on SynHX validator coverage to the flows that the
  pinned client and current wireframe server both actually support today:
  successful login, failed-auth gating, and XOR login. Rationale: pretending
  that file listing, file download, chat, or news are covered would obscure the
  real protocol gaps. Date/Author: 2026-04-12 / Assistant.

- Decision: keep roadmap item 1.6.2 open after this implementation pass.
  Rationale: the harness plumbing, CI fail-closed path, and supported login
  coverage are now in place, but genuine real-client validation for file/news/
  chat flows still depends on additional compatibility or server feature work.
  Date/Author: 2026-04-12 / Assistant.

## Outcomes & Retrospective

- Implemented: shared SynHX installation for devboxer and CI, explicit
  wireframe server binary resolution, reusable validator helper modules, stable
  login and XOR-login coverage, validator Makefile targets, sqlite CI
  execution, and design/developer documentation updates.

- Did not implement: genuine real-client coverage for file download, chat, or
  news, and did not produce usable SynHX file-list validation beyond the
  request-side compatibility tolerance.

- Follow-up work: add compatibility for the legacy SynHX file-list and news
  transaction shapes, or introduce a different real client that matches the
  current wireframe protocol surface well enough to cover the remaining flows.

- Lesson: harness plumbing and protocol overlap are separate delivery tracks.
  The validator can now fail closed and target the right server explicitly, but
  that does not by itself create coverage for flows the pinned client and
  server still express differently.

## Context and orientation

Primary files and modules in current state:

- `docs/roadmap.md`: source of roadmap item 1.6.2 and its acceptance wording.
- `docs/design.md`: contains the validator-harness design narrative and must
  record final decisions taken during implementation.
- `docs/users-guide.md`: must be reviewed for any user-visible runtime or
  workflow changes.
- `README.md`: currently documents `cd validator && cargo test` with `hx`
  installed, and may need updated validator instructions.
- `validator/tests/login.rs`: current login-only `hx` validator test.
- `validator/tests/xor_compat.rs`: current XOR/news validator test plus the
  Helix-detection guard.
- `validator/src/lib.rs`: likely home for extracted harness primitives.
- `test-util/src/server/mod.rs`: shared server launcher that already defaults to
  `mxd-wireframe-server`.
- `.github/workflows/ci.yml`: current CI workflow that lacks `hx` provisioning
  and validator execution.
- `scripts/install-synhx.sh`: shared SynHX installer for devboxer and CI.
- `Cargo.toml`: workspace/default-member settings that currently exclude the
  validator crate from default runs.
- `Makefile`: current quality-gate entrypoints, which likewise omit explicit
  validator targets.

## Plan of work

### Stage A: lock scope against real capabilities

Audit roadmap item 1.6.2 against the actual wireframe implementation and the
current `hx` client surface.

Work items:

- Inventory which requested flows are actually implemented in the wireframe
  server today.
- Verify the exact `hx` commands and observable prompts for login, news, file
  download, and chat using the pinned client version installed by
  `scripts/install-synhx.sh`.
- Produce a short support matrix:
  requested flow, server support status, `hx` support status, and whether the
  flow is unblockable within 1.6.2.
- If chat or file download still require future roadmap features, record that
  explicitly in `docs/design.md` and keep 1.6.2 open until scope is resolved.

Validation gate for Stage A:

- A support matrix is captured in the Decision Log or design document.
- No implementation proceeds under a false assumption that the named flows
  already exist.

### Stage B: refactor the validator harness into testable primitives

Move one-off helper logic out of individual validator tests and into small
modules under `validator/src/`.

Target responsibilities:

- `hx` binary discovery, version validation, and Helix rejection;
- PTY/session creation with configurable expect timeouts;
- explicit server-launch configuration that selects `mxd-wireframe-server`;
- common command helpers (`connect`, `login`, `post news`, `download file`,
  chat actions if supported);
- temporary directory and downloaded-file assertions;
- shared skip-vs-fail policy so local missing-`hx` runs stay informative while
  CI fails closed when provisioning is expected.

Unit coverage to add with `rstest`:

- happy: valid `hx` discovery and command/session setup;
- unhappy: missing binary, Helix binary, invalid prompt, failed cleanup;
- edge: explicit server-binary override, temp-path handling, CI policy toggle.

Validation gate for Stage B:

- `cargo test -p validator` passes for harness-unit coverage without needing a
  live `hx` session for every code path.

### Stage C: add behavioural coverage where it helps

Introduce `rstest-bdd` scenarios for validator behaviour that benefits from
Given/When/Then readability, while keeping low-level PTY mechanics in ordinary
tests.

Candidate BDD surfaces:

- deciding whether the harness should skip or fail based on environment and CI
  expectations;
- proving explicit wireframe-server targeting and setup preconditions;
- documenting multi-step validation contracts for download/chat orchestration,
  if those flows are available.

Validation gate for Stage C:

- Behaviour scenarios cover happy and unhappy orchestration paths and remain
  deterministic without depending on fragile prompt timing for every step.

### Stage D: extend end-to-end validator flows

Expand the real-client validator coverage in `validator/tests/`.

Required flows:

- login:
  successful login, invalid credentials, and compatibility-sensitive prompts or
  responses;
- news:
  list and/or post/read coverage with unhappy-path assertions such as missing
  article or permission denial where the server supports them;
- file download:
  download a known fixture to a temp directory and assert byte-for-byte output,
  plus at least one unhappy path such as missing file or privilege failure;
- chat:
  validate the currently accepted chat scope with one happy path and one
  unhappy path, which may require multiple `hx` sessions if the underlying
  transaction support exists.

Implementation notes:

- Prefer parameterized `rstest` cases to avoid duplicating backend or flow
  scaffolding.
- Reuse `test-util` database setup helpers where possible.
- Keep assertions on observable client behaviour first, then verify persisted
  side effects (for example, downloaded bytes or stored news body) where needed
  for confidence.

Validation gate for Stage D:

- Validator flow tests cover happy, unhappy, and relevant edge cases for each
  accepted flow and point explicitly at `mxd-wireframe-server`.

### Stage E: wire the validator into developer workflows and CI

Make validator execution an intentional, documented part of the repository
tooling.

Work items:

- Add Makefile targets such as `test-validator-sqlite` and
  `test-validator-postgres`, or an equivalent target structure that keeps the
  commands discoverable.
- Decide whether validator tests should use `cargo test` instead of
  `cargo nextest` because of PTY/process semantics, and document the reason.
- Extend `.github/workflows/ci.yml` with a validator job or matrix leg that:
  runs `scripts/install-synhx.sh` to provision a pinned SynHX `hx` binary,
  verifies it is not Helix, runs the validator harness against the wireframe
  server, and preserves logs/artifacts useful for failure diagnosis.
- Ensure the installer script works in both devboxer and CI by supporting
  environment overrides for the binary install directory and source checkout
  path, while defaulting to `/usr/local/bin` and `~/git`.
- Keep local Postgres validator execution documented via
  `pg-embed-setup-unpriv`, even if CI initially runs only the SQLite-backed
  validator job.

Validation gate for Stage E:

- CI contains at least one non-skipping validator execution path and fails when
  the harness cannot run as configured.

### Stage F: documentation, roadmap closure, and final verification

Update documentation and close the roadmap item only after evidence exists.

Documentation tasks:

- `docs/design.md`: record final decisions about explicit wireframe targeting,
  `hx` provisioning, skip/fail policy, and any accepted scope adjustment.
- `docs/users-guide.md`: update only if server behaviour or a documented
  validation workflow becomes user-relevant.
- `README.md`: refresh validator run instructions if commands or prerequisites
  change.
- `docs/roadmap.md`: mark 1.6.2 done with date and concise completion summary
  only after all acceptance criteria and CI evidence exist.

Validation tasks:

- run formatter, lints, and tests through tee-captured commands;
- include validator-specific commands in the final gate if they remain outside
  `make test`;
- run Markdown validation because this task changes planning and likely updates
  design/user documentation.

## Validation and commands

Expected implementation-time commands, to be adjusted only if tooling changes:

```sh
./scripts/install-synhx.sh
make fmt
make check-fmt
make lint
make test
make markdownlint
make nixie
cargo test -p validator --features sqlite
cargo test -p validator --no-default-features --features postgres
```

Local Postgres enablement should follow
`docs/pg-embed-setup-unpriv-users-guide.md`, for example:

```sh
./scripts/install-synhx.sh
export PG_VERSION_REQ=15.13.0
export PG_RUNTIME_DIR="$PWD/.tmp/pg/runtime"
export PG_DATA_DIR="$PWD/.tmp/pg/data"
export PG_SUPERUSER=postgres
export PG_PASSWORD=postgres
export PG_TEST_BACKEND=embedded
cargo install --locked --version 0.5.0 pg-embed-setup-unpriv
pg_embedded_setup_unpriv
cargo test -p validator --no-default-features --features postgres
```

All long-running quality-gate commands should be run with `set -o pipefail` and
piped through `tee` to retain complete logs for review.
