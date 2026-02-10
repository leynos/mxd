# Publish an internal Hotline and SynHX compatibility matrix

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: TODO

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 1.5.3 requires an internal compatibility matrix that documents
supported clients, known deviations, and required toggles. The matrix must live
in `docs/` and be referenced during release-note QA sign-off.

Success is observable when:

- a dedicated matrix document exists in `docs/` and is easy to find from
  operator-facing documentation;
- matrix entries are backed by automated `rstest` unit tests and
  `rstest-bdd` behavioural scenarios that cover happy paths, unhappy paths, and
  edge cases;
- `docs/design.md` records the design decisions behind matrix structure and
  evidence sources;
- `docs/users-guide.md` reflects any user-visible compatibility behaviour;
- roadmap entry 1.5.3 is marked done with a date and short completion note.

## Constraints

- Keep compatibility quirks in wireframe adapter code only. Domain modules must
  remain free of client-specific branching.
- Treat roadmap 1.5.2 outputs as baseline truth:
  - SynHX is identified via handshake sub-version `2`.
  - Hotline 1.8.5 vs 1.9 is identified from login field 160.
  - Login banner fields 161/162 are omitted for SynHX and included for Hotline.
- The matrix document must be under `docs/` and written for internal QA use.
- The matrix must cite evidence sources already present in this repository
  (tests, protocol docs, design notes) so claims are auditable.
- Use `rstest` for unit tests and `rstest-bdd` for behavioural scenarios where
  applicable.
- Local Postgres-backed validation must use `pg_embedded_setup_unpriv` before
  running full test gates.
- No new external dependencies without explicit escalation.
- Markdown must satisfy `make fmt`, `make markdownlint`, and `make nixie`.
- Keep prose in en-GB-oxendict spelling and wrap text to 80 columns.

## Tolerances (exception triggers)

- Scope: if implementation exceeds 16 changed files or 500 net LOC, stop and
  escalate with options.
- Release-note linkage ambiguity: if no durable, reviewable release-note
  location can be identified for QA sign-off evidence, stop and request a
  product-owner decision before proceeding.
- Behaviour mismatch: if matrix claims conflict with existing tests or protocol
  docs, stop and resolve the discrepancy before publishing.
- Dependencies: if a new crate is required, stop and escalate.
- Rework loop: if lint/test gates fail twice after attempted fixes, stop and
  escalate with captured logs.

## Risks

- Risk: matrix becomes stale as compatibility logic evolves.
  Severity: medium. Likelihood: medium. Mitigation: derive every matrix row
  from code paths and attach explicit test identifiers so future edits fail
  loudly when behaviour changes.
- Risk: release-note reference requirement is fulfilled informally and cannot be
  audited later. Severity: medium. Likelihood: medium. Mitigation: define a
  concrete release-note reference point and document the QA sign-off step in
  repository docs or workflow guidance.
- Risk: matrix overstates support for unimplemented transactions.
  Severity: high. Likelihood: medium. Mitigation: add a status column that
  distinguishes "implemented and tested" from "planned / not yet implemented".
- Risk: test coverage focuses only on happy paths.
  Severity: medium. Likelihood: medium. Mitigation: add explicit unhappy/edge
  scenario cases for unknown client versions and XOR-disabled flows.

## Progress

- [x] (2026-02-10 00:00Z) Draft ExecPlan from roadmap 1.5.3, existing
  compatibility implementation, and testing/documentation guides.
- [ ] Confirm and implement release-note QA sign-off reference mechanism.
- [ ] Create the internal compatibility matrix document in `docs/`.
- [ ] Add or extend `rstest` unit tests that back each matrix row.
- [ ] Add or extend `rstest-bdd` scenarios for happy/unhappy/edge paths.
- [ ] Update `docs/design.md` and `docs/users-guide.md`.
- [ ] Mark roadmap item 1.5.3 as done.
- [ ] Run quality gates and capture logs.

## Surprises & Discoveries

- `docs/roadmap.md` requires release-note QA sign-off linkage, but the
  repository currently has no dedicated release-notes document.
- Compatibility behaviour for SynHX and Hotline variants is already codified in
  `src/wireframe/compat_policy.rs`, `src/wireframe/compat.rs`, and behavioural
  suites `tests/wireframe_login_compat.rs` and `tests/wireframe_xor_compat.rs`.
- `docs/users-guide.md` already describes XOR detection and login field gating,
  which reduces user-guide delta but still requires matrix cross-linking.

## Decision Log

- Decision: place the matrix at `docs/internal-compatibility-matrix.md`.
  Rationale: keeps roadmap acceptance explicit ("lives in docs") and avoids
  mixing matrix data into broad narrative docs. Date/Author: 2026-02-10 / Codex.
- Decision: matrix rows must include evidence pointers to concrete tests.
  Rationale: prevents drift and supports QA auditability. Date/Author:
  2026-02-10 / Codex.
- Decision: treat release-note reference mechanism as an explicit deliverable,
  not an implicit process assumption. Rationale: roadmap acceptance requires
  traceable QA sign-off linkage. Date/Author: 2026-02-10 / Codex.

## Outcomes & Retrospective

Pending implementation.

Expected outcome: a maintained internal compatibility matrix with automated
evidence, release-note QA linkage, and roadmap closure for 1.5.3.

## Context and orientation

Current compatibility behaviour is implemented and tested in:

- `src/wireframe/compat_policy.rs` (`ClientCompatibility` classification and
  login reply augmentation).
- `src/wireframe/compat.rs` (`XorCompatibility` detection and payload rewrite).
- `tests/wireframe_login_compat.rs` with
  `tests/features/wireframe_login_compat.feature`.
- `tests/wireframe_xor_compat.rs` with
  `tests/features/wireframe_xor_compat.feature`.

Current documentation references:

- `docs/roadmap.md` (task 1.5.3 acceptance and dependency on 1.5.2).
- `docs/design.md` (handshake metadata, compatibility policy, XOR behaviour).
- `docs/users-guide.md` (operator-facing runtime behaviour).
- `docs/protocol.md` (login version and banner field semantics).
- `docs/pg-embed-setup-unpriv-users-guide.md` (local Postgres setup).
- `docs/rust-testing-with-rstest-fixtures.md` and
  `docs/rstest-bdd-users-guide.md` (test patterns).
- `docs/reliable-testing-in-rust-via-dependency-injection.md` (deterministic
  tests and isolation).

## Plan of work

### Stage A: Define matrix scope and release-note linkage

Confirm the matrix schema and QA evidence model before writing content. At
minimum include:

- client identifier;
- handshake/login markers used for detection;
- supported transactions or feature slices;
- known deviations from canonical Hotline expectations;
- required client or server toggles;
- evidence links to automated tests.

Then define how release notes reference the matrix during QA sign-off. If a
repository-native release-notes location is absent, create a durable reference
point (for example a release-note template or documented QA checklist step).

### Stage B: Create matrix document under `docs/`

Add `docs/internal-compatibility-matrix.md` with internal-facing scope,
explicitly distinguishing:

- supported now (implemented + tested),
- partially supported with known deviations,
- not yet implemented but planned in roadmap.

Each row must include at least one evidence pointer (unit test, behavioural
scenario, or protocol clause). Include a short glossary for client names and
toggle terminology.

### Stage C: Back matrix claims with `rstest` unit coverage

Extend unit coverage where gaps exist so each matrix claim has deterministic
proof. Prioritize:

- client classification boundaries (`Unknown`, `Hotline85`, `Hotline19`,
  `SynHx`);
- login reply augmentation expectations for each class;
- XOR detection transitions (disabled to enabled and non-trigger cases).

Ensure unhappy and edge cases are explicit, such as unknown or low login
version values and payloads that should not trigger XOR mode.

### Stage D: Back matrix claims with `rstest-bdd` behavioural coverage

Add or extend BDD scenarios so external behaviour aligns with matrix rows:

- happy: Hotline 1.8.5 and 1.9 login replies include banner fields;
- happy: SynHX-compatible XOR login/news paths succeed with automatic
  compatibility enablement;
- unhappy: unknown client classification omits compatibility extras;
- edge: XOR-encoded payloads in unsupported contexts do not corrupt non-text
  fields and report expected errors.

Use existing `WireframeBddWorld` fixtures and keep scenarios isolated.

### Stage E: Update design and user-facing docs

Update `docs/design.md` with the matrix design decisions:

- where the matrix lives;
- what evidence qualifies a matrix claim;
- how deviations are tracked as implementation evolves.

Update `docs/users-guide.md` to link to the matrix and document any operator
actions required to interpret compatibility status or client toggles.

### Stage F: Close roadmap item 1.5.3

Mark roadmap entry 1.5.3 as done in `docs/roadmap.md` with:

- completion date;
- concise summary of what shipped (matrix location + QA linkage);
- confirmation that dependency 1.5.2 was satisfied.

### Stage G: Run verification and quality gates

Use `pg_embedded_setup_unpriv` before full test gates, then run documentation
and Rust quality gates with `tee` logs for auditability.

## Concrete steps

1. Confirm existing behaviour and evidence gaps.

   - Review:
     - `src/wireframe/compat_policy.rs`
     - `src/wireframe/compat.rs`
     - `tests/wireframe_login_compat.rs`
     - `tests/wireframe_xor_compat.rs`
   - Record missing unhappy/edge coverage in the Decision Log before editing.

2. Add matrix document.

   - Create `docs/internal-compatibility-matrix.md`.
   - Include supported clients, known deviations, required toggles, and evidence
     references.

3. Extend automated tests where claims lack proof.

   - Unit (`rstest`) updates in:
     - `src/wireframe/compat_policy.rs`
     - `src/wireframe/compat.rs`
   - Behavioural (`rstest-bdd`) updates in:
     - `tests/wireframe_login_compat.rs`
     - `tests/wireframe_xor_compat.rs`
     - `tests/features/wireframe_login_compat.feature`
     - `tests/features/wireframe_xor_compat.feature`

4. Update documentation.

   - `docs/design.md` with matrix governance/evidence decisions.
   - `docs/users-guide.md` with matrix cross-reference and operator guidance.
   - Add or update release-note QA reference point so sign-off explicitly cites
     `docs/internal-compatibility-matrix.md`.

5. Mark roadmap task done.

   - Update task 1.5.3 in `docs/roadmap.md` to checked state with completion
     note.

6. Prepare local Postgres environment.

   - Run:

     ```sh
     cargo install --locked pg-embed-setup-unpriv
     pg_embedded_setup_unpriv
     ```

7. Run quality gates with logs.

   - Use `set -o pipefail` for each command and capture logs:

     ```sh
     PROJECT="$(basename "$(pwd)")"
     BRANCH="$(git branch --show)"

     make fmt 2>&1 | tee "/tmp/fmt-${PROJECT}-${BRANCH}.out"
     make markdownlint 2>&1 | tee "/tmp/markdownlint-${PROJECT}-${BRANCH}.out"
     make nixie 2>&1 | tee "/tmp/nixie-${PROJECT}-${BRANCH}.out"
     make check-fmt 2>&1 | tee "/tmp/check-fmt-${PROJECT}-${BRANCH}.out"
     make lint 2>&1 | tee "/tmp/lint-${PROJECT}-${BRANCH}.out"
     make test 2>&1 | tee "/tmp/test-${PROJECT}-${BRANCH}.out"
     ```

## Validation and acceptance

Acceptance is met when all statements below are true:

- `docs/internal-compatibility-matrix.md` exists and documents supported
  clients, known deviations, and required toggles.
- Release-note QA sign-off guidance references
  `docs/internal-compatibility-matrix.md`.
- Unit tests (`rstest`) and behavioural tests (`rstest-bdd`) cover happy,
  unhappy, and edge cases that substantiate matrix rows.
- `docs/design.md` records matrix-related design decisions.
- `docs/users-guide.md` reflects any behaviour/UI impacts relevant to users.
- Roadmap item 1.5.3 is marked done with date and summary.
- `make fmt`, `make markdownlint`, `make nixie`, `make check-fmt`, `make lint`,
  and `make test` all pass.

## Idempotence and recovery

- Matrix and docs edits are idempotent; rerunning formatting and linting should
  produce no diff after first success.
- If Postgres setup fails, rerun `pg_embedded_setup_unpriv` and verify required
  environment variables from `docs/pg-embed-setup-unpriv-users-guide.md`.
- If tests fail, update matrix claims or compatibility logic so both
  implementation and docs agree before re-running gates.

## Artifacts and notes

Capture and retain:

- quality-gate logs under `/tmp/*-${PROJECT}-${BRANCH}.out`;
- diff showing matrix, docs, and roadmap updates;
- test output demonstrating compatibility scenarios and edge-case handling.

## Interfaces and dependencies

Expected touched interfaces (if coverage gaps require code changes):

- `crate::wireframe::compat_policy::ClientCompatibility`
- `crate::wireframe::compat::XorCompatibility`
- BDD fixtures in `test-util/src/wireframe_bdd_world.rs`

No external dependency additions are expected.
