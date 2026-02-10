# Verify XOR and sub-version compatibility logic with Kani

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETED (2026-02-10)

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 1.5.4 requires bounded formal verification of the existing
wireframe compatibility logic added in 1.5.1 and 1.5.2. After this work, we
will be able to run Kani harnesses that prove two high-value properties without
panics:

- XOR text-field transformations are involutive for bounded payloads
  (encode/decode round-trip).
- Client compatibility gating remains correct for bounded handshake sub-version
  and login-version inputs.

Success is observable when:

- new Kani harnesses pass with `cargo kani` for XOR and version-gating
  invariants;
- targeted `rstest` unit tests and `rstest-bdd` behavioural tests cover happy,
  unhappy, and edge paths around the same invariants;
- design and verification documentation records the verification decisions;
- `docs/users-guide.md` reflects any user-visible behaviour changes (or
  explicitly confirms none);
- roadmap entry 1.5.4 is marked done with completion date and summary.

## Constraints

- Preserve compatibility semantics delivered by roadmap items 1.5.1 and 1.5.2:
  - XOR mode is adapter-local and per-connection.
  - SynHX classification is driven by handshake `sub_version == 2`.
  - Hotline 1.8.5/1.9 classification is derived from login field 160.
- Do not move compatibility quirks into domain modules; keep all logic at the
  wireframe adapter boundary (`src/wireframe/*`).
- Keep Kani harnesses adjacent to production logic under `#[cfg(kani)]`.
- Add or extend `rstest` unit tests and `rstest-bdd` behavioural tests where
  applicable for happy, unhappy, and edge paths.
- Use local Postgres setup via `pg_embedded_setup_unpriv` as documented in
  `docs/pg-embed-setup-unpriv-users-guide.md` before full test gates.
- No new external dependencies unless escalation is approved.
- Keep documentation in en-GB-oxendict style and wrap prose at 80 columns.
- Respect file-size guardrail (no source file over 400 lines); split modules if
  needed.

## Tolerances (exception triggers)

- Scope: if implementation exceeds 14 files changed or 550 net LOC, stop and
  escalate with options.
- Interfaces: if any public API outside `src/wireframe` must change, stop and
  escalate.
- Dependencies: if any new crate or cargo feature is needed, stop and escalate.
- Verification feasibility: if Kani cannot prove target invariants after two
  bounded-harness refinements, stop and report alternative proof decomposition.
- Test-loop limit: if gates still fail after two correction passes, stop and
  escalate with logs.
- Ambiguity: if roadmap acceptance can be interpreted in conflicting ways,
  pause and ask for direction before coding.

## Risks

- Risk: Kani may struggle with atomics or heavier payload decode/encode paths
  inside compatibility wrappers. Severity: medium. Likelihood: medium.
  Mitigation: prove pure, bounded helper invariants and keep adapter-level
  wrappers covered by `rstest`/BDD regression tests.

- Risk: refactoring for proofability can accidentally change runtime behaviour.
  Severity: medium. Likelihood: medium. Mitigation: test-first on existing
  behaviour and maintain BDD coverage before and after refactors.

- Risk: acceptance criteria focus on Kani while behavioural regressions are
  missed. Severity: high. Likelihood: low. Mitigation: require full `make test`
  pass and explicit behavioural scenario updates for boundary cases.

- Risk: docs drift between verification strategy and roadmap state.
  Severity: low. Likelihood: medium. Mitigation: update `docs/design.md`,
  `docs/verification-strategy.md`, `docs/users-guide.md`, and `docs/roadmap.md`
  in the same change.

## Progress

- [x] (2026-02-10 20:31Z) Reviewed roadmap 1.5.4 acceptance criteria and
  dependency on 1.5.2.
- [x] (2026-02-10 20:31Z) Collected current compatibility implementation
  context from `src/wireframe/compat.rs`, `src/wireframe/compat_policy.rs`, and
  routing glue.
- [x] (2026-02-10 20:31Z) Reviewed existing verification patterns in
  `src/wireframe/codec/kani.rs`, `src/transaction/reader/kani.rs`, and
  `src/header_util/kani.rs`.
- [x] (2026-02-10 20:31Z) Drafted this ExecPlan.
- [x] (2026-02-10 22:57Z) Implemented bounded Kani harnesses for XOR
  round-trip and client-kind gating invariants:
  `kani_xor_bytes_round_trip_bounded`,
  `kani_xor_payload_round_trip_text_fields_bounded`,
  `kani_client_kind_sub_version_precedence`, and
  `kani_login_extras_boundary_gate`.
- [x] (2026-02-10 22:57Z) Added and extended `rstest` unit tests and
  `rstest-bdd` behavioural scenarios for happy, unhappy, and boundary
  compatibility paths.
- [x] (2026-02-10 22:57Z) Updated `docs/design.md`,
  `docs/verification-strategy.md`, `docs/users-guide.md`, and `docs/roadmap.md`.
- [x] (2026-02-10 23:10Z) Ran formatting, lint, test, docs, and Kani
  verification gates; all passed.

## Surprises & Discoveries

- The referenced Postgres setup guide path in the request uses
  `pg-embedded-...`; in this repository the file is
  `docs/pg-embed-setup-unpriv-users-guide.md`.
- Existing Kani coverage is present for framing and header helpers, but there
  is no Kani coverage yet for `XorCompatibility` or `ClientCompatibility`.
- Existing `rstest-bdd` suites already cover primary compatibility flows in
  `tests/features/wireframe_xor_compat.feature` and
  `tests/features/wireframe_login_compat.feature`; this reduces implementation
  risk and allows focused edge-case additions.
- `Makefile` currently has no dedicated Kani target; Kani proofs run via
  explicit `cargo kani --harness ...` commands.
- Kani exploration initially stalled when wrapper-level XOR harnessing touched
  transitive internals using randomness and hash set allocation paths.
- The environment's `/data` filesystem was full during heavy builds, so
  `CARGO_TARGET_DIR` and `CARGO_INCREMENTAL=0` were required for reliable local
  gate execution.

## Decision Log

- Decision: Keep Kani harnesses adjacent to compatibility code by adding
  `#[cfg(kani)] mod kani;` for `compat` and `compat_policy` modules, mirroring
  existing harness organization. Rationale: preserves locality of invariants
  and avoids widening visibility of internal helpers. Date/Author: 2026-02-10 /
  Codex.

- Decision: Prefer proving bounded pure invariants (classification and XOR
  transformations) and rely on runtime tests for wrapper orchestration.
  Rationale: improves Kani tractability and keeps proof obligations precise.
  Date/Author: 2026-02-10 / Codex.

- Decision: Treat behavioural tests as regression proof for externally visible
  routing outcomes, even though 1.5.4 is verification-focused. Rationale:
  requested coverage includes behavioural testing where applicable.
  Date/Author: 2026-02-10 / Codex.

- Decision: Prove payload transform invariants through
  `xor_params`-centred bounded harnesses instead of wrapper entry points when
  Kani enters transitive randomness-heavy internals. Rationale: preserves
  acceptance intent while keeping proofs tractable and deterministic.
  Date/Author: 2026-02-10 / Codex.

- Decision: Keep runtime panic behaviour for impossible payload length overflow
  in `header_util`, but guard the branch under `#[cfg(kani)]` with
  `kani::assume(false)` plus abort for verifier compatibility. Rationale:
  maintains runtime semantics while avoiding Kani panic instrumentation issues.
  Date/Author: 2026-02-10 / Codex.

## Outcomes & Retrospective

Roadmap item 1.5.4 was implemented and validated end-to-end.

What shipped:

- Added Kani harness modules for XOR compatibility and sub-version/login
  compatibility gating.
- Added `rstest` unit coverage for XOR unhappy-path decode failures, login
  version boundaries, SynHX precedence, idempotent augmentation, and
  unparseable login payload handling.
- Added `rstest-bdd` behavioural scenarios for boundary login gating and SynHX
  precedence.
- Updated design, verification strategy, and users guide documentation.
- Marked roadmap item 1.5.4 as done.

Verification outcomes:

- Kani harnesses passed for all target invariants.
- Full repository quality gates passed (`make check-fmt`, `make lint`,
  `make test`, `make markdownlint`, and `make nixie`).

Key retrospective note:

- Kani proofability improved materially when harnesses targeted pure bounded
  helpers and avoided payload wrapper paths that transitively reached entropy
  or heavier library internals.

## Context and orientation

Compatibility logic currently lives in:

- `src/wireframe/compat.rs` (`XorCompatibility`, XOR detection and
  payload encode/decode shims).
- `src/wireframe/compat_policy.rs` (`ClientCompatibility`, login version
  capture, and login reply augmentation policy).
- `src/wireframe/routes/mod.rs` (hooks that call
  `record_login_payload`, `augment_login_reply`, and XOR decoding path).

Existing behavioural coverage:

- `tests/wireframe_xor_compat.rs` with
  `tests/features/wireframe_xor_compat.feature`.
- `tests/wireframe_login_compat.rs` with
  `tests/features/wireframe_login_compat.feature`.

Existing Kani pattern references:

- `src/wireframe/codec/kani.rs`.
- `src/transaction/reader/kani.rs`.
- `src/header_util/kani.rs`.

Documentation touchpoints for closure:

- `docs/design.md` for design/verification decisions.
- `docs/verification-strategy.md` for harness inventory and run commands.
- `docs/users-guide.md` for user-visible behaviour notes (if behaviour changes)
  or an explicit no-change confirmation.
- `docs/roadmap.md` to mark 1.5.4 done.

## Plan of work

### Stage A: lock invariants and proof boundaries

Document the exact bounded invariants to prove and map each invariant to one
code location. Proposed scope:

- XOR involution on text fields (`xor_bytes(xor_bytes(x)) == x`) for bounded
  byte arrays.
- XOR payload round-trip for bounded text-bearing parameter payloads.
- Client kind classification precedence (SynHX by handshake sub-version takes
  priority over login-version thresholds).
- Login extras gating (`should_include_login_extras`) boundary correctness.

Exit criteria: invariant list and harness boundaries recorded in Decision Log
and reflected in `docs/design.md` during implementation.

### Stage B: scaffold Kani harness modules

Add `#[cfg(kani)]` module declarations and harness files:

- `src/wireframe/compat.rs` -> `src/wireframe/compat/kani.rs`.
- `src/wireframe/compat_policy.rs` -> `src/wireframe/compat_policy/kani.rs`.

Keep harnesses small and bounded; avoid complex async/runtime paths and focus
on deterministic helper-level proofs.

Exit criteria: harnesses compile under `cargo kani` with stable symbol names.

### Stage C: add/expand deterministic test coverage

Add or refine `rstest` unit tests for boundary and unhappy paths not currently
covered, including:

- XOR detection non-trigger for non-text payloads and already-valid UTF-8.
- Login-version boundary cases (`150`, `151`, `189`, `190`, `u16::MAX`).
- SynHX precedence over login-version thresholds.
- Augmentation idempotence when fields 161/162 already exist.

Extend `rstest-bdd` scenarios where applicable to include at least one edge
boundary scenario for login-version gating and one unhappy XOR path regression.

Exit criteria: unit and behavioural tests demonstrate happy/unhappy/edge
coverage tied to acceptance criteria.

### Stage D: run Kani proofs and project gates

Run bounded harness proofs and then full repository gates:

- `cargo kani` for each new harness.
- Formatting/lint/test suites via Make targets.

Exit criteria: all commands complete successfully with logged outputs.

### Stage E: update documentation and roadmap

Update docs with final decisions and completion evidence:

- `docs/design.md`: compatibility verification design decisions.
- `docs/verification-strategy.md`: new harness names and rationale.
- `docs/users-guide.md`: mention any user-visible behaviour changes; if none,
  add a concise no-behaviour-change note under compatibility guidance.
- `docs/roadmap.md`: mark 1.5.4 as done with date and summary.

Exit criteria: documentation and roadmap match implemented state and tests.

## Concrete steps

1. Create harness modules and wire them into existing compatibility modules.

2. Implement bounded Kani harnesses, for example:

   - `kani_xor_bytes_round_trip_bounded`
   - `kani_xor_payload_round_trip_text_fields_bounded`
   - `kani_client_kind_sub_version_precedence`
   - `kani_login_extras_boundary_gate`

   Final names may vary but must be listed in `docs/verification-strategy.md`
   after implementation.

3. Add or extend unit tests in:

   - `src/wireframe/compat.rs`
   - `src/wireframe/compat_policy.rs`

4. Add or extend behavioural scenarios in:

   - `tests/wireframe_login_compat.rs`
   - `tests/features/wireframe_login_compat.feature`
   - `tests/wireframe_xor_compat.rs`
   - `tests/features/wireframe_xor_compat.feature`

5. Prepare local Postgres prerequisites:

   ```sh
   PG_VERSION_REQ="=16.4.0" \
   PG_RUNTIME_DIR="/var/tmp/pg-embedded-setup-unpriv/install" \
   PG_DATA_DIR="/var/tmp/pg-embedded-setup-unpriv/data" \
   PG_SUPERUSER="postgres" \
   PG_PASSWORD="postgres_pass" \
   cargo run --release --bin pg_embedded_setup_unpriv
   ```

6. Run documentation formatting/validation gates with logs:

   ```sh
   PROJECT_NAME="$(command -v get-project >/dev/null 2>&1 && get-project || basename "$PWD")"
   BRANCH_NAME="$(git branch --show)"

   make fmt 2>&1 | tee "/tmp/fmt-${PROJECT_NAME}-${BRANCH_NAME}.out"
   make markdownlint 2>&1 | tee "/tmp/markdownlint-${PROJECT_NAME}-${BRANCH_NAME}.out"
   make nixie 2>&1 | tee "/tmp/nixie-${PROJECT_NAME}-${BRANCH_NAME}.out"
   ```

7. Run Rust quality gates with logs:

   ```sh
   make check-fmt 2>&1 | tee "/tmp/check-fmt-${PROJECT_NAME}-${BRANCH_NAME}.out"
   make lint 2>&1 | tee "/tmp/lint-${PROJECT_NAME}-${BRANCH_NAME}.out"
   make test 2>&1 | tee "/tmp/test-${PROJECT_NAME}-${BRANCH_NAME}.out"
   ```

8. Run Kani harnesses with logs:

   ```sh
   cargo kani -p mxd --harness <kani_harness_name> \
     2>&1 | tee "/tmp/kani-${PROJECT_NAME}-${BRANCH_NAME}.out"
   ```

9. Update documentation and mark roadmap item 1.5.4 complete.

## Validation and acceptance

Roadmap 1.5.4 is accepted when:

- Kani harnesses prove XOR encode/decode round-trips and sub-version/version
  gating properties for bounded inputs.
- Proof runs complete without panics.
- Unit tests (`rstest`) and behavioural tests (`rstest-bdd`) cover happy,
  unhappy, and edge paths for the touched compatibility behaviour.
- `make check-fmt`, `make lint`, `make test`, `make markdownlint`, and
  `make nixie` pass.
- `docs/design.md`, `docs/users-guide.md`, and `docs/roadmap.md` are updated
  consistently.

## Idempotence and recovery

- All steps are re-runnable.
- If Postgres setup fails, clean and re-run only the staging directories under
  `/var/tmp/pg-embedded-setup-unpriv`.
- If Kani fails due to bound complexity, reduce harness scope to smaller pure
  helpers, record the decision, and rerun.
- Do not mark roadmap complete until all gates and documentation updates pass.

## Artifacts and notes

Capture and retain:

- Kani proof summaries per harness.
- `make test` summary for sqlite, postgres, wireframe-only, and verification
  suites.
- A short diff summary showing roadmap status update and verification-doc
  updates.

## Interfaces and dependencies

Expected code interfaces touched:

- `crate::wireframe::compat::XorCompatibility` and related helper functions.
- `crate::wireframe::compat_policy::ClientCompatibility` and classification
  helpers.
- New `#[cfg(kani)]` submodules under wireframe compatibility modules.

No new dependencies are expected.

## Revision note

Initial draft created from current roadmap, compatibility code, existing Kani
patterns, and testing/documentation guidance. Updated on 2026-02-10 with
implementation outcomes, final decisions, and gate results.
