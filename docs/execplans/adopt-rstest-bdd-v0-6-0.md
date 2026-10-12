# Adopt rstest-bdd v0.6.0

This ExecPlan is a living document. Keep `Constraints`, `Tolerances`, `Risks`,
`Progress`, `Surprises & discoveries`, `Decision log`,
`Outcomes & retrospective`, `Conformance basis`, and `Verification plan`
current.

Status: IN PROGRESS

## Purpose / big picture

Move MXD's existing behavioural tests from rstest-bdd 0.5.0 to the published
0.6.0 release without changing application behaviour or losing scenario
coverage. The imported upstream guides remain byte-for-byte copies from the
verified tag commit. Success is observable through the existing scenario lanes,
focused migration regressions, the compile-time UI tests, and green repository
and hosted gates. The user authorized a squash merge after the published PR
candidate reaches review and CI equilibrium; release publication is outside
scope.

## Constraints

- Keep the two imported guides byte-for-byte equal to the files at upstream
  commit `72fb22635670e456545ca368805ba4c1c9d7bd69`.
- Use only APIs published in rstest-bdd 0.6.0. In particular, preserve the
  `#[from(rstest_bdd_harness_context)]` spelling and do not assume v0.7
  borrowing or lifecycle guarantees.
- Preserve existing scenario assertions, state isolation, feature choices,
  compile-time validation, and intentionally external Tokio tests for steps
  that yield `Pending`.
- Do not modify vendored or submodule source, suppress a failed assertion,
  disable validation, or introduce compatibility facades for the old private
  test API.
- Keep the existing nightly/compiler policy unless a tested dependency floor
  requires a change. Do not claim an untested stable MSRV.
- Use the named branch `adopt-rstest-bdd-v0-6-0` and verify the exact PR head
  before each publication or merge action.

## Tolerances (exception triggers)

- Scope: stop if the migration needs production behaviour changes or more than
  30 owned files beyond the planned migration and validation evidence.
- Interface: stop if a non-test public MXD API must change.
- Dependencies: stop before adding a dependency other than the harness or
  compile-time test support needed by the existing tests.
- Validation: keep any failing or unavailable check visible; do not merge with
  a required check red, pending, or unknown.
- Review: do not bypass a missing current-candidate review, an unresolved
  substantive finding, or the managed CodeRabbit route and cooldown.

## Risks

- Risk: upstream documentation at the 0.6.0 tag also discusses later beta and
  v0.7 APIs. Severity: high; likelihood: medium. Mitigation: use the pinned
  source and published 0.6.0 package to resolve discrepancies; keep later APIs
  out of the implementation.
- Risk: migration regressions can expose old false-green scenarios or fixture
  binding mistakes. Severity: high; likelihood: medium. Mitigation: compare
  scenario inventories and retain focused alias, fixture override, error, and
  async-suspension tests.
- Risk: compile-time UI tests invoke nested Cargo builds and may exceed the
  repository's ordinary per-test timeout on a cold coverage build. Severity:
  medium; likelihood: medium. Mitigation: give only the UI-test package a
  longer Nextest timeout and keep its pass and compile-fail assertions active.
- Risk: local resource pressure can make long database tests fail without a
  deterministic product defect. Severity: medium; likelihood: medium.
  Mitigation: retain raw command results and bounded-lane evidence; never
  replace the aggregate failure with a claim that it passed.

## Progress

- [x] (2026-10-02) Inspect the clean baseline, repository instructions,
  workspace manifests, compiler policy, BDD inventory, and existing migration
  PR before editing.
- [x] (2026-10-02) Fetch tag `v0.6.0`, confirm its resolution equals the
  surveyed immutable commit, import both requested guides, read both guides,
  and record checksums and relative-link mapping.
- [x] (2026-10-11) Align the live BDD dependencies and migrate existing test
  consumers, preserving feature choices and async execution semantics.
- [x] (2026-10-11) Add runtime regressions for migration-specific fixture,
  aliased-result, returned-value, and yielding-step contracts; add compile-time
  pass/fail coverage for scenario selection and harness generation.
- [x] (2026-10-11) Run local validation and publish candidate
  `a9c422137d9560b48236714b574ee678e7bc7868` to PR #616.
- [x] (2026-10-12) Add a package-scoped Nextest timeout for the hosted coverage
  timeout and run the local candidate gates sequentially. SQLite coverage
  passed 633 tests with one skip, including the trybuild test in 103.816
  seconds. Formatting, lint, typecheck, documentation, and locked-metadata
  gates passed. The first aggregate `make test` attempt failed during embedded
  PostgreSQL startup before later subtargets; the failed scenario passed alone
  with checkout-specific PostgreSQL directories.
- [x] (2026-10-12) Reconcile CodeRabbit's prior compile-time/UI finding on the
  current candidate; the bot confirmed the pass/fail fixtures and workspace
  coverage wiring. The new current-head review then found that the package was
  still absent from the ordinary `make test` target and the CI Test step, and
  that the trybuild fixture crates and developer guide needed explicit
  documentation.
- [x] Add the UI package to the aggregate Make target and CI Test step, document
  that route and timeout, and add module docs to both trybuild fixtures.
- [x] Correct the two-digit source gutter in the trybuild snapshot and run the
  candidate gates sequentially. The final run passed formatting, lint,
  typecheck, spelling, Markdown, Mermaid, lockfile, and all tests. PostgreSQL
  passed 497 tests, SQLite 502, SQLite-plus-TOML 500, UI contracts 1, and the
  later suites passed; one existing test was skipped and four doctests were
  ignored. The first two snapshot adjustments failed on exact indentation; the
  final expected diagnostic matches rustc output.
- [ ] Commit the validated repair, publish it, and verify remote parity and
  fresh hosted checks.
- [ ] Observe current-head CodeRabbit review and all review surfaces, reconcile
  findings, and merge only after the accepted candidate reaches equilibrium.
- [ ] When all merge conditions hold, squash-merge PR #616 guarded by its
  accepted full head SHA and verify the landed commit and post-merge jobs.

## Surprises & discoveries

- Observation: the upstream tag resolves exactly to the surveyed commit, and
  the required imports contain later-release-labelled sections. Evidence:
  `docs/rstest-bdd-v0-6-0-provenance.md` records the tag, commit, checksums,
  and link map. Impact: later sections are preserved as source text but are not
  treated as v0.6 implementation requirements.
- Observation: the normal local SQLite lane passed all 502 tests at two
  threads, but an unrestricted aggregate run first encountered an OS thread
  creation error and a later serial aggregate run timed out in a PostgreSQL
  scenario. Evidence and logs are recorded in the provenance notes and PR
  description. Impact: these outcomes remain visible and are not called green.
- Observation: hosted SQLite coverage ran 633 tests with 632 passing and one
  existing skip, then terminated the new trybuild test at the default 60-second
  Nextest limit. Evidence: CI run `38179181706`, attempt 1, coverage job
  `114587896461`, log bundle
  `/tmp/mxd-pr-checks-a9c4221-BrDHLO/run-38179181706-attempt-1/failed.log`.
  Impact: add a narrow 300-second override and rerun the candidate; do not
  weaken or omit UI coverage.
- Observation: the local aggregate test run failed while starting the embedded
  PostgreSQL cluster, before the scenario reached its schema assertions.
  Running that scenario with checkout-specific `PG_RUNTIME_DIR` and
  `PG_DATA_DIR` passed. The documented shared per-user cluster directory is a
  plausible collision point, but the original failure's precise cause is not
  established. Impact: retain the aggregate failure as a failed gate and use
  the passing isolated scenario only as focused diagnostic evidence.
- Observation: the CodeRabbit status context is successful but says
  `Review paused`; its prior review describes the old `d929998` range.
  Evidence: PR #616's current status and edited walkthrough comment. Impact:
  current review completion remains unproven until the review state is
  reconciled.
- Observation: the completed CodeRabbit review on `b4eba6d` found the trybuild
  package was not in `make test` or the CI Test step, and that its two fixture
  modules and developer-facing package instructions lacked documentation.
  Evidence: review `5485872832`, commit
  `b4eba6d4450b29f632ed1ae86715d571371a5da9`. Impact: wire the package into
  both ordinary test paths and document the 300-second package timeout before
  the next candidate gate run.
- Observation: the first gate run after that repair found the trybuild
  diagnostic snapshot differed only in rustc's indentation for a two-digit
  source line; the first snapshot adjustment left one extra space before the
  `11 |` gutter on the next run. The PostgreSQL and SQLite suites passed in
  both runs. Evidence: the first captured candidate log is
  `/tmp/test-dca659dd-c303-4df0-b113-2d19de1540c9-adopt-rstest-bdd-v0-6-0.out`;
  the second is
  `/tmp/test-dca659dd-c303-4df0-b113-2d19de1540c9-adopt-rstest-bdd-v0-6-0-2.out`.
  Impact: match the exact compiler diagnostic and validate the resulting
  candidate through a fresh sequential gate pass.
- Observation: after removing the extra leading space before `11 |`, all
  sequential gates passed, including the trybuild compile-fail contract.
  Evidence: final test log
  `/tmp/test-dca659dd-c303-4df0-b113-2d19de1540c9-adopt-rstest-bdd-v0-6-0-3.out`.
  Impact: the workspace test target and CI lane now exercise the documented UI
  contracts on the validated candidate.

## Decision log

- Decision: import upstream material from the immutable commit rather than a
  moving branch. Rationale: the migration brief requires exact published
  release provenance. Date/Author: 2026-10-02 / Codex.
- Decision: keep compatible synchronous scenarios synchronous and use external
  Tokio tests for genuinely yielding steps. Rationale: the v0.6 Tokio harness
  runs synchronous scenario closures. Date/Author: 2026-10-11 / Codex.
- Decision: add trybuild pass and compile-fail cases for the changed macro
  surface. Rationale: the pre-merge UI coverage check identified an actual
  compile-time test gap. Date/Author: 2026-10-11 / Codex.
- Decision: scope a 300-second Nextest timeout to `rstest-bdd-ui-tests` after
  hosted coverage hit the 60-second limit. Rationale: trybuild compiles fixture
  crates inside the test, and the other 632 SQLite tests passed. This changes
  the runner's patience for one compile test, not its assertions or coverage.
  Date/Author: 2026-10-12 / Codex.

## Outcomes & retrospective

The dependency and source migration is implemented, and the imported guide
provenance is recorded separately from the untouched upstream copies. The
scenario inventory is 18 feature files and 83 declarations at the candidate,
versus 17 files and 82 declarations at the rebased base; the one addition is
the focused migration feature. Local formatting, lint, typecheck,
documentation, lock, and SQLite coverage checks passed, including the
compile-contract test without snapshot regeneration. The aggregate local test
target failed at embedded PostgreSQL startup; the isolated failed scenario
passed with unique cluster directories, but that does not replace the aggregate
result. The plan remains in progress until hosted coverage passes at the new
head, current review surfaces are reconciled, and the authorized merge is
verified.

## Context and orientation

The root `Cargo.toml` owns the workspace dependency requirements and includes
`tests/rstest-bdd-ui`. `tests/` contains the migrated consumers, existing
feature scenarios, migration regressions, and the trybuild harness. The Nextest
policy lives in `.config/nextest.toml`; CI runs SQLite and PostgreSQL coverage
through `.github/workflows/ci.yml`. The imported guide checksums, tag
resolution, baseline inventory, applicability matrix, compiler history, and
detailed gate record are in `docs/rstest-bdd-v0-6-0-provenance.md`. No
repository Terms of Reference, technical design, or ADR governs this test
dependency migration; the accepted migration brief and the pinned upstream
release documentation are the conformance sources.

The BDD regression specification in
`tests/features/rstest_bdd_migration.feature` is:

```gherkin
Feature: BDD migration contracts
  Scenario: Yielding steps retain fixture overrides
    When migration work suspends before returning a replacement
    Then migration observes replacement 7
```

## Conformance basis

- `MIG-DOC`: the two v0.6.0 guides are exact imports from upstream tag
  `v0.6.0`, commit `72fb22635670e456545ca368805ba4c1c9d7bd69`. Trace:
  `MIG-DOC -> EP-M1 -> provenance checksums and link map`.
- `MIG-API`: update only live consumers to published 0.6.0 APIs and exclude
  v0.6.1 beta/v0.7 behaviour. Trace:
  `MIG-API -> EP-M2 -> Cargo metadata, source regressions, locked graph`.
- `MIG-BEHAVIOUR`: preserve existing test intent, scenario isolation, async
  suspension, and coverage. Trace:
  `MIG-BEHAVIOUR -> EP-M2/EP-M3 -> feature inventory and test lanes`.
- `MIG-DELIVERY`: validate the exact published branch and merge only after
  equilibrium. Trace:
  `MIG-DELIVERY -> EP-M4 -> current-head CI/review and guarded merge readback`.

## Verification plan

No production API, domain invariant, persistent format, or wire protocol is
introduced by this migration. The relevant obligations are test contracts and
repository-owned runner configuration:

- Obligation `V-DOC`: imported files equal the immutable upstream blobs.
  Method: SHA-256 comparison recorded in the provenance document. Domain:
  exactly the two requested guide paths. Artefact: the two imported guides and
  `docs/rstest-bdd-v0-6-0-provenance.md`. Evidence: provenance checksum check;
  both hashes must match the source blobs. Non-vacuity: the two non-empty
  upstream blobs are independently read from the commit.
- Obligation `V-BDD`: migration scenario preserves a fixture override across
  an actual suspension. Method: executable Gherkin scenario. Domain: one
  yielding step and replacement value 7. Artefact:
  `tests/rstest_bdd_migration.rs` and
  `tests/features/rstest_bdd_migration.feature`. Evidence: SQLite and
  wireframe-only scenario lanes pass. Non-vacuity: the step yields before
  returning a replacement, and the assertion requires the replacement value.
- Obligation `V-UI`: supported named scenario/harness code compiles, while a
  missing scenario is rejected with the expected semantic diagnostic. Method:
  trybuild compile-pass and compile-fail fixtures. Domain: one valid named
  scenario with canonical Tokio harness and one invalid scenario selection.
  Artefact: `tests/rstest-bdd-ui/tests/ui.rs` and
  `tests/rstest-bdd-ui/tests/ui/`. Evidence:
  `cargo test --locked -p rstest-bdd-ui-tests` and SQLite coverage.
  Non-vacuity: the pass fixture is compiled; the fail fixture must fail and its
  checked diagnostic must match. No fault injection is needed because the
  invalid fixture is the negative control.
- Obligation `V-TIMEOUT`: the nested UI compile is not killed by the default
  60-second limit under coverage, without changing its assertions. Method:
  package-scoped Nextest override plus the exact hosted SQLite coverage lane.
  Domain: the sole integration test in `rstest-bdd-ui-tests`; override is 300
  seconds and remains below the default profile's 10-minute global timeout.
  Artefact: `.config/nextest.toml`. Evidence: local SQLite coverage passes the
  same test in 103.816 seconds; a passing current-head hosted coverage job is
  still required. Non-vacuity: the same UI test must appear and pass in each
  coverage result; exclusion or ignore is a failure.
- Obligation `V-MATRIX`: backend and optional-feature behaviour remains covered.
  Method: repository CI feature matrix and local named Make targets. Domain:
  PostgreSQL, SQLite, wireframe-only, validation, Loom, and verification
  configurations actually used by CI. Artefacts: Makefile and CI workflow.
  Evidence: current-head hosted checks and sequential local gates. Non-vacuity:
  verify each named job/target result rather than relying on `--all-features`.

External axioms: Cargo resolves crates according to the registry's published
package metadata; rstest-bdd 0.6.0 provides the documented macro and harness
contracts; Nextest applies the scoped override to the named package; hosted
services report results for the exact checked head. Tests do not claim to
verify these tools' internal implementations. The compile-fail snapshot is a
diagnostic contract and may need a deliberate review if compiler wording
changes.

## Plan of work

### EP-M1: establish source and baseline

Inspect repository instructions, every maintained Cargo manifest and lockfile,
BDD consumers, features, compiler pins, CI commands, and scenario inventories.
Fetch and verify the release tag, import and read both guides, and record
provenance. This milestone is complete; see the provenance document.

### EP-M2: migrate existing consumers

Align the inherited dependency requirements and lockfile to 0.6.0, make only
the relevant source and fixture changes, and add regression scenarios for
changed 0.6 contracts. Preserve application APIs and async semantics. This
milestone is complete and is recorded in the migration evidence.

### EP-M3: verify macro expansion

Add a compile-pass fixture for named scenario selection with the canonical
Tokio harness and a compile-fail fixture for a missing scenario. Run both
without regenerating the expected diagnostic. This milestone is implemented;
hosted coverage revealed that its nested compile needs a larger scoped test
timeout.

### EP-M4: converge and deliver

In `.config/nextest.toml`, add a 300-second slow timeout only for package
`rstest-bdd-ui-tests`. Confirm the effective configuration, run all required
local gates sequentially, commit and push the validated change, and observe the
new current-head checks. Reconcile CodeRabbit's UI warning and review status,
CodeScene, all review threads, and the full check set. If all are clear, squash
merge using the accepted full head SHA and verify the landing commit and
post-merge jobs.

Conformance check: preserve all requested release, behaviour, and provenance
requirements; the runner override changes no test assertion or scenario; the
v0.6 API boundary remains intact; the override introduces no dependency, public
interface, or trust-boundary change; trace links above remain current.
Recovery: revert only the scoped override if it causes unrelated suites to
exceed the profile's global timeout, then diagnose the compile workload rather
than excluding the test. Remaining gaps: current hosted coverage is red and
CodeRabbit review completion is unverified.

## Concrete steps

Run from the repository root. The targeted configuration check must show a
300-second timeout for the UI package while the general default remains 60
seconds:

```sh
cargo nextest show-config test-groups
cargo test --locked -p rstest-bdd-ui-tests
```

Then run the Makefile's candidate gates sequentially, preserving each exit
status and log. The canonical Rust and documentation gates include
`make check-fmt`, `make lint`, `make typecheck`, `make test`,
`make -o spelling markdownlint`, and `make nixie`, followed by the feature and
specialized targets listed in `docs/rstest-bdd-v0-6-0-provenance.md`. A hosted
coverage result must include the UI test as passed. Do not merge while any
required check or review state remains red, pending, or unknown.

## Validation and acceptance

The migration is accepted when the inherited dependency graph resolves to the
published 0.6.0 family without duplicate BDD runtime or macro versions; both
imports still match their checksums; all previous feature scenarios remain
covered with only the documented migration scenario added; pass/fail compile
contracts hold; the scoped timeout allows coverage to complete; local gates and
all applicable hosted checks pass; CodeRabbit's warning and review state are
reconciled; and PR #616 is verified merged into `main`.

Existing evidence includes the successful focused UI test and green local
format, lint, typecheck, documentation, dependency, validator, Loom, and
specialized feature gates. The prior aggregate `make test` failures remain
disclosed in provenance and the PR body, with their successful bounded-lane
counter-evidence. The new timeout adjustment still needs focused and complete
candidate validation; no green result is inferred from that earlier evidence.

## Idempotence and recovery

The Nextest override is idempotent and affects only the UI package. Re-running
the focused test does not alter snapshots. If trybuild reports a changed
diagnostic, inspect the semantic difference before updating the snapshot; do
not use `TRYBUILD=overwrite` as validation. If a validation target fails, keep
its log, fix the cause, and run that gate again only after the change. Do not
cancel another build or bypass compiler admission.

## Interfaces and dependencies

The migration adds no production-facing interface. Existing tests use the
published `rstest-bdd` and macro crates, plus the canonical
`rstest-bdd-harness-tokio` path where a synchronous Tokio harness is needed.
`trybuild` is confined to the non-published UI-test package. The Nextest
timeout override changes only test-runner policy for that package.

## Revision note

2026-10-12: Added this branch-named ExecPlan to record the completed migration
and current delivery evidence. Added the hosted coverage timeout discovery and
the scoped Nextest adjustment as the remaining implementation milestone.
