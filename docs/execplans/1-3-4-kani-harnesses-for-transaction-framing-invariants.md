# Add Kani harnesses for transaction framing invariants

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md: not present in the repository root.

## Purpose / Big Picture

Add bounded Kani proofs for transaction framing invariants so that panic
freedom and correctness can be demonstrated for the core wire protocol framing
logic. Success means `cargo kani` verifies header validation, fragment sizing,
and transaction ID echoing for bounded inputs, while existing unit and
behavioural tests continue to pass without changing observable server behaviour.

## Constraints

- Do not change the wire protocol framing semantics defined in
  `docs/protocol.md`.
- Keep Kani harnesses adjacent to the code under test under `#[cfg(kani)]`, per
  `docs/verification-strategy.md`.
- New modules must begin with `//!` documentation and remain under 400 lines.
- Preserve the hexagonal boundary described in `docs/design.md`; do not leak
  `wireframe` types into domain code.
- Avoid `unsafe` and new lint suppressions; follow existing clippy policies.
- Any documentation edits must follow
  `docs/documentation-style-guide.md` and wrap prose at 80 columns.

## Tolerances (Exception Triggers)

- Scope: more than 10 files touched or more than 400 lines (net) changed.
- Interface: any public API signature change outside the framing modules.
- Dependencies: adding the `kani` dev-dependency is in scope; any other new
  dependency or feature flag requires escalation. The `rstest-bdd` bump to
  0.3.2 is approved; further dependency changes require escalation.
- Iterations: if tests or Kani proofs fail after two fix attempts, escalate.
- Ambiguity: if multiple valid interpretations of the invariants would change
  the harness design, stop and ask.
- Time: any single stage taking more than two hours of active work.

## Risks

    - Risk: Kani struggles with async or `bincode` encoding paths.
      Severity: medium
      Likelihood: medium
      Mitigation: keep harnesses on pure helpers and small synchronous
      encode/parse helpers.
    - Risk: Bounds chosen for Kani are too small to be meaningful.
      Severity: medium
      Likelihood: low
      Mitigation: document bounds in `docs/design.md` and align them with
      `MAX_FRAME_DATA`/`MAX_PAYLOAD_SIZE` where feasible.
    - Risk: Dependency version drift between Cargo.toml and docs.
      Severity: low
      Likelihood: medium
      Mitigation: update docs alongside any dependency bumps and re-run
      format/lint gates.

## Progress

    - [x] (2026-01-11 00:00Z) Draft plan with current invariants and sources.
    - [x] (2026-01-11 00:00Z) Upgrade rstest-bdd to 0.3.2 and align docs.
    - [x] (2026-01-12 00:00Z) Begin implementation and confirm framing targets.
    - [x] (2026-01-12 00:00Z) Add Kani harness modules and fragmentation helper.
    - [x] (2026-01-12 00:00Z) Add `rstest` unit coverage for fragment ranges.
    - [x] (2026-01-12 00:00Z) Update design/verification docs and guide text.
    - [x] (2026-01-12 00:00Z) Run formatting, lint, tests, Kani proofs, and
      update roadmap status.

## Surprises & Discoveries

    - Observation: `cfg(kani)` triggered the `unexpected_cfgs` lint under
      `-D warnings`.
      Evidence: `cargo build -p mxd` warned about unexpected `cfg(kani)` until
      `Cargo.toml` was updated with `check-cfg`.
      Impact: Added `unexpected_cfgs` configuration to allow `cfg(kani)`.
    - Observation: Kani requires `kani::assert` to include a message and
      needed an explicit unwind bound for fragment range iteration.
      Evidence: `cargo kani` reported missing argument errors and produced
      unbounded loop unwinding for `fragment_ranges` until assertions were
      updated and `#[kani::unwind(3)]` was added.
      Impact: Added descriptive messages and bounded the harness loop to keep
      proofs tractable.

## Decision Log

    - Decision: Place Kani harnesses in `#[cfg(kani)]` modules alongside
      framing code (`src/transaction`, `src/wireframe/codec`, `src/header_util`).
      Rationale: aligns with `docs/verification-strategy.md` and keeps harness
      access to private helpers without widening visibility.
      Date/Author: 2026-01-11, Codex
    - Decision: Upgrade `rstest-bdd` to 0.3.2 and update documentation samples.
      Rationale: user confirmed the version bump; docs must reflect the active
      dependency.
      Date/Author: 2026-01-11, Codex
    - Decision: Factor `fragment_ranges` from the codec encoder to make
      fragmentation proofs reusable in Kani and unit tests.
      Rationale: keeps the fragment sizing invariant centralised and avoids
      duplicating the framing loop.
      Date/Author: 2026-01-12, Codex
    - Decision: Bound the fragment sizing harness to two fragments and set
      `#[kani::unwind(3)]`.
      Rationale: keeps Kani proof time bounded while still covering multi-
      fragment cases.
      Date/Author: 2026-01-12, Codex

## Outcomes & Retrospective

- Implemented Kani harnesses covering header validation, fragment sizing, and
  reply header ID echoing with bounded inputs.
- Added reusable fragment range iterator plus `rstest` coverage.
- Ran lint, tests, and Kani proofs; all passed with warnings limited to
  unsupported Kani constructs outside reachable paths.

## Context and Orientation

Transaction framing logic is split across:

- `src/transaction/frame.rs` for `FrameHeader`, `parse_transaction`, and frame
  size limits.
- `src/transaction/reader/mod.rs` and
  `src/transaction/reader/streaming.rs` for header validation and continuation
  sizing (`validate_first_header`, `validate_continuation_frame`).
- `src/wireframe/codec/mod.rs` for `HotlineTransaction` encoding/decoding and
  header validation (`validate_header`, `validate_fragment_consistency`).
- `src/header_util.rs` for `reply_header`, which must echo transaction IDs.
- Behaviour coverage lives in `tests/features/wireframe_transaction.feature`,
  `tests/features/wireframe_transaction_encoding.feature`,
  `tests/features/transaction_streaming.feature`, and
  `tests/features/wireframe_routing.feature`, wired via `rstest-bdd`.

Verification guidance and tooling expectations are in
`docs/verification-strategy.md`. Documentation and testing conventions are
described in `docs/rust-testing-with-rstest-fixtures.md`,
`docs/rstest-bdd-users-guide.md`, `docs/rust-doctest-dry-guide.md`, and
`docs/pg-embedded-setup-unpriv-users-guide.md`.

## Plan of Work

Stage A: confirm invariants and harness targets. Review the framing helpers in
`src/transaction` and `src/wireframe/codec` to map which invariants correspond
to “header validation”, “fragment sizing”, and “transaction ID echoing”. If the
invariants imply refactoring for testability (for example, extracting a pure
fragment sizing helper), record the decision in `docs/design.md` before
changing code.

Stage B: add supporting unit and behaviour tests. Introduce `rstest` unit tests
for any new helper predicates or reply header behaviour. Confirm existing
behaviour-driven development (BDD) scenarios cover the behaviours; add new
`.feature` scenarios only if a gap is found (for example, a missing fragment
sizing edge case).

Stage C: implement Kani harnesses. Add `#[cfg(kani)]` modules adjacent to the
framing code. Use bounded payload sizes (for example, a small constant like 64
or 128 bytes) and `kani::assume` to constrain header values. Prove:

- header validation accepts exactly the valid combinations,
- fragment sizing never exceeds `MAX_FRAME_DATA` and sums to `total_size`, and
- `reply_header` echoes request IDs for bounded payload lengths without panic.

Stage D: documentation and roadmap. Update `docs/design.md` with any new
verification decisions and bounds. Update `docs/verification-strategy.md` with
the new harness names and run instructions if they change. Only update
`docs/users-guide.md` if user-visible behaviour changed (likely none). Mark
roadmap item 1.3.4 as done once Kani proofs and tests pass.

Each stage must end with its validation step before moving on. This plan is
draft-only; implementation requires explicit approval.

## Concrete Steps

1. Inspect framing helpers and existing tests:

       rg -n "validate_first_header|validate_header|reply_header" src
       rg -n "wireframe_transaction|transaction_streaming" tests/features

2. Add Kani harness scaffolding and any helper predicates:

   - Add `#[cfg(kani)] mod kani;` blocks in the relevant modules.
   - If a new helper is extracted, add `rstest` unit tests alongside it.

3. Implement Kani harnesses with bounded inputs:

   - `src/transaction/reader/mod.rs` (or a sibling `kani.rs`) for header
     validation invariants.
   - `src/wireframe/codec/mod.rs` for fragment sizing during encode.
   - `src/header_util.rs` for reply ID echoing and panic freedom with bounded
     payload lengths.

4. Update documentation:

   - `docs/design.md` for verification decisions and bounds.
   - `docs/verification-strategy.md` for harness names/run commands.
   - `docs/users-guide.md` only if behaviour changed.

5. Run documentation formatting and linting (use `tee` for logs):

       make fmt 2>&1 | tee /tmp/mxd-fmt.log
       make markdownlint 2>&1 | tee /tmp/mxd-markdownlint.log
       make nixie 2>&1 | tee /tmp/mxd-nixie.log

6. Run Rust formatting and tests (use `pg_embedded_setup_unpriv` for
   PostgreSQL):

       cargo install --locked pg-embed-setup-unpriv
       pg_embedded_setup_unpriv 2>&1 | tee /tmp/pg-embed.log
       make check-fmt 2>&1 | tee /tmp/mxd-check-fmt.log
       make lint 2>&1 | tee /tmp/mxd-lint.log
       make test 2>&1 | tee /tmp/mxd-test.log

7. Run Kani proofs (confirm harness names first):

       cargo kani -p mxd --harness <harness_name> 2>&1 | tee /tmp/mxd-kani.log

8. Mark roadmap item 1.3.4 as done in `docs/roadmap.md` once all checks pass.

## Validation and Acceptance

Acceptance from the roadmap:

- Kani proves header validation, fragment sizing, and transaction ID echoing
  for bounded payloads without panics.

Quality criteria:

- Tests: `make test` passes (sqlite, postgres, wireframe-only).
- Lint/format: `make check-fmt`, `make lint`, and doc formatting/linting all
  pass.
- Verification: `cargo kani -p mxd --harness <names>` passes for the new
  harnesses with bounded inputs.

Evidence of success:

- New `rstest` unit tests covering helper predicates/reply headers pass.
- Existing `rstest-bdd` scenarios for framing and routing continue to pass, or
  new scenarios cover any newly identified gap.
- Roadmap item 1.3.4 updated to done.

## Idempotence and Recovery

All steps are repeatable. Re-running `pg_embedded_setup_unpriv` is safe and
idempotent per its guide. If Kani or test commands fail, revert only the local
changes since the last green state and re-run the failing step. Do not proceed
to roadmap updates until all validations succeed.

## Artifacts and Notes

Keep short transcripts of:

    - `cargo kani -p mxd --harness <harness_name>` output (success summary).
    - `make test` summary lines for each feature set.

## Interfaces and Dependencies

Expected additions:

- `kani` dev-dependency in the root `Cargo.toml` (version compatible with the
  installed Kani toolchain).
- `#[cfg(kani)]` harnesses such as:

  - `kani_validate_first_header_matches_predicate` in
      `src/transaction/reader/kani.rs`.
  - `kani_validate_continuation_frame_matches_predicate` in
      `src/transaction/reader/kani.rs`.
  - `kani_validate_header_matches_predicate` in
      `src/wireframe/codec/kani.rs`.
  - `kani_fragment_ranges_cover_payload` in `src/wireframe/codec/kani.rs`.
  - `kani_reply_header_echoes_id` in `src/header_util/kani.rs`.

Each harness should:

- Use bounded arrays (for example `[u8; KANI_MAX]`) and a selected length to
  avoid unbounded `Vec` sizes.
- Use `kani::assume` to constrain `FrameHeader` fields and payload lengths.
- Assert invariants with `kani::assert` without panicking.

## Revision note

Initial draft created to cover roadmap item 1.3.4 and verification
requirements. Updated to reflect the approved rstest-bdd 0.3.2 upgrade.
