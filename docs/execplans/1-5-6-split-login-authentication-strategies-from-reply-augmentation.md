# Split login authentication strategies from reply augmentation (roadmap 1.5.6)

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: IN PROGRESS (implementation underway; documentation now records current
outcomes and remaining wiring work)

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 1.5.6 requires the login path to separate authentication strategy
selection from reply augmentation while keeping existing Hotline and SynHX
behaviour unchanged. After this work, `WireframeRouter` and
`CompatibilityLayer` remain the only guardrail routing path, but they will wire
`AuthStrategy` and `LoginReplyAugmenter` explicitly so future SynHX hashed-auth
and HOPE reply fields can be added without entangling responsibilities.

Success is observable when:

- `AuthStrategy` and `LoginReplyAugmenter` are wired through the guardrail
  routing entrypoint (`src/wireframe/router.rs`,
  `src/wireframe/compat_layer.rs`);
- login defaults remain unchanged for Hotline 1.8.5/1.9 and SynHX
  classification/gating;
- unit tests (`rstest`) and behavioural tests (`rstest-bdd`) cover happy,
  unhappy, and edge paths for the split responsibilities;
- local postgres-backed validation is run with `pg_embedded_setup_unpriv`;
- design/user documentation is updated, and roadmap item 1.5.6 is marked done
  only after all gates pass.

## Constraints

- Preserve roadmap 1.5.5 guardrail contract:
  `WireframeRouter` stays the only public routing entrypoint and
  `CompatibilityLayer` remains the compatibility orchestration surface.
- Preserve current externally visible login compatibility behaviour:
  - SynHX (`handshake.sub_version == 2`) omits banner fields.
  - Hotline 1.8.5/1.9 include banner fields using the current version gating.
  - Existing login success and failure semantics remain unchanged.
- Keep compatibility-specific strategy/augmenter wiring in wireframe adapter
  modules; do not leak client-quirk logic into domain-layer abstractions.
- Use `rstest` and `rstest-bdd` for new coverage where applicable.
- Use `pg_embedded_setup_unpriv` for local postgres test setup before full
  verification gates.
- Avoid new dependencies unless escalation is approved.
- Keep documentation in en-GB-oxendict and wrap markdown prose at 80 columns.
- Do not mark roadmap item 1.5.6 done until implementation and all required
  gates are complete.

## Tolerances (exception triggers)

- Scope: if implementation exceeds 18 changed files or 700 net LOC, stop and
  escalate with options.
- Interface: if public APIs outside wireframe compatibility/routing surfaces
  must change, stop and escalate.
- Dependency: if a new crate or cargo feature is required, stop and escalate.
- Ambiguity: if ADR-003 requirements can be interpreted in materially different
  ways (especially where authentication executes), pause and get direction.
- Rework loop: if required gates fail after two fix passes, stop and escalate
  with logs.

## Risks

- Risk: architectural leakage from adapter concerns into shared command/domain
  modules while introducing `AuthStrategy`. Severity: high. Likelihood: medium.
  Mitigation: keep strategy selection at guardrail boundary and document any
  unavoidable cross-boundary shim explicitly.
- Risk: behaviour regressions for Hotline/SynHX login replies while refactoring
  augmentation into a dedicated interface. Severity: high. Likelihood: medium.
  Mitigation: preserve existing BDD scenarios and add boundary/unhappy tests.
- Risk: hook-order regressions in routing guardrails. Severity: medium.
  Likelihood: medium. Mitigation: retain and extend spy-based order assertions.
- Risk: postgres test setup flakiness in local environments. Severity: medium.
  Likelihood: low. Mitigation: stage cluster with `pg_embedded_setup_unpriv`
  and use deterministic fixture setup.

## Agent team and ownership

Implementation uses a coordinated agent team with explicit ownership:

- Architecture agent:
  finalizes `AuthStrategy` and `LoginReplyAugmenter` contracts, selection
  rules, and boundary-safe wiring points using ADR-002/ADR-003 constraints.
- Implementation agent:
  applies code changes in wireframe routing/compatibility modules and any
  minimal command/login integration shim required to execute selected strategy.
- Verification agent:
  owns `rstest` + `rstest-bdd` additions, edge-case coverage, and full gate
  execution with log capture.
- Documentation agent:
  updates `docs/design.md`, `docs/users-guide.md`, and `docs/roadmap.md`
  completion state after verification evidence exists.

Handoff rule: each agent must leave concrete file/line evidence and command
outputs before the next stage starts.

## Context and orientation

Current relevant implementation state:

- `src/wireframe/router.rs` routes frames via
  `on_request -> dispatch -> on_reply` and finalizes replies through
  `compat_layer::finalize_reply`.
- `src/wireframe/compat_layer.rs` currently performs request decode/login
  metadata recording and reply augmentation directly through
  `ClientCompatibility::augment_login_reply`.
- `src/wireframe/compat_policy.rs` contains client classification,
  login-version recording, and current augmentation logic.
- `src/login.rs` contains current login authentication flow
  (`handle_login`), invoked through `src/commands/handlers.rs`.
- Behavioural compatibility coverage already exists in
  `tests/wireframe_login_compat.rs` and
  `tests/wireframe_compat_guardrails_bdd.rs`.

Documentation and requirements anchors:

- `docs/roadmap.md` item 1.5.6 acceptance and dependency on 1.5.5.
- `docs/adr-003-login-authentication-and-reply-augmentation.md`.
- `docs/adr-002-compatibility-guardrails-and-augmentation.md`.
- `docs/design.md` compatibility and guardrail sections.
- `docs/protocol.md` login response expectations.
- `docs/users-guide.md` operator-visible wireframe compatibility behaviour.
- `docs/verification-strategy.md` for verification deliverables.
- `docs/rust-testing-with-rstest-fixtures.md` and
  `docs/rstest-bdd-users-guide.md` for test style and fixture usage.
- `docs/pg-embed-setup-unpriv-users-guide.md` for postgres setup.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` and
  `docs/rust-doctest-dry-guide.md` for deterministic tests and docs tests.
- `docs/wireframe-users-guide.md` and `docs/ortho-config-users-guide.md` as
  broader platform context (no direct behavioural change expected for 1.5.6).

## Plan of work

### Stage A: lock interface boundaries and default behaviour contract

- Define adapter-level interfaces and defaults:
  - `AuthStrategy`: login authentication workflow contract.
  - `LoginReplyAugmenter`: login reply decoration contract.
- Codify default implementations that preserve current behaviour.
- Define strategy/augmenter selection based on existing compatibility metadata,
  but keep default mapping for Hotline/SynHX unchanged in 1.5.6.
- Record decisions and invariants in `docs/design.md` before code merge.

Exit criteria:

- interface names and responsibilities match ADR-003;
- no user-visible behaviour changes versus current baseline;
- design decisions recorded.

### Stage B: wire interfaces into guardrail routing entrypoint

- Update wireframe guardrail routing so `CompatibilityLayer` owns or resolves
  both `AuthStrategy` and `LoginReplyAugmenter`.
- Ensure login request flow and login reply flow each call the correct
  interface and remain deterministic.
- Preserve existing guardrail ordering assertions (`on_request -> dispatch ->
  on_reply`) or update spy expectations only when logically required and proven.
- Keep `WireframeRouter` as sole public entrypoint.

Exit criteria:

- acceptance wiring requirement is satisfied in code;
- routing path remains centralized and testable;
- default Hotline/SynHX behaviour remains unchanged.

### Stage C: unit coverage with rstest (happy, unhappy, edge)

Add or extend unit tests for:

- strategy selection and fallback by `ClientKind` (`Hotline85`, `Hotline19`,
  `SynHx`, `Unknown`);
- default auth strategy success/failure parity with existing login logic;
- reply augmenter idempotence and banner-field gating invariants;
- hook-order invariants with strategy/augmenter participation;
- edge boundaries for login version values (150, 151, 189, 190, `u16::MAX`).

Prefer parameterized `#[rstest]` cases over duplicated tests.

### Stage D: behavioural coverage with rstest-bdd

Extend BDD scenarios and step bindings to prove end-to-end behaviour through
`mxd-wireframe-server`:

- happy: Hotline login still succeeds and includes expected banner fields;
- happy: SynHX login still succeeds and omits banner fields;
- unhappy: invalid credentials fail without unintended augmentation;
- edge: compatibility precedence (`sub_version == 2` over login-version gate)
  remains unchanged after strategy/augmenter split.

Maintain existing feature files unless new dedicated scenarios improve clarity.

### Stage E: documentation updates

- `docs/design.md`: record architecture decisions for strategy/augmenter
  separation, ownership, and guardrail wiring.
- `docs/users-guide.md`: update wireframe compatibility section. If behaviour is
  unchanged, state this explicitly and describe the internal refactor at a
  user-appropriate level.
- `docs/roadmap.md`: mark 1.5.6 done with date and concise completion summary
  only after all gates below pass.

### Stage F: verification and quality gates

Run local postgres setup first, then full gates. Capture logs with `tee`. Use
this log pattern: `/tmp/$ACTION-$(get-project)-$BRANCH.out`, where
`BRANCH=$(git branch --show)`.

Recommended sequence:

1. Prepare postgres runtime:

   ```sh
   cargo run --release --bin pg_embedded_setup_unpriv \
     | tee /tmp/pg-setup-$(get-project)-$BRANCH.out
   ```

2. Run formatting checks:

   ```sh
   make check-fmt | tee /tmp/check-fmt-$(get-project)-$BRANCH.out
   ```

3. Run lint checks:

   ```sh
   make lint | tee /tmp/lint-$(get-project)-$BRANCH.out
   ```

4. Run test suites:

   ```sh
   make test | tee /tmp/test-$(get-project)-$BRANCH.out
   ```

5. Validate markdown:

   ```sh
   make markdownlint | tee /tmp/markdownlint-$(get-project)-$BRANCH.out
   ```

6. Validate Mermaid diagrams:

   ```sh
   make nixie | tee /tmp/nixie-$(get-project)-$BRANCH.out
   ```

7. Run type checks:

   ```sh
   make typecheck | tee /tmp/typecheck-$(get-project)-$BRANCH.out
   ```

If `typecheck` is unavailable in future branches, record explicit evidence
before skipping.

## Concrete implementation checklist

1. Add strategy/augmenter interfaces and default implementations in wireframe
   compatibility modules.
2. Wire both interfaces into `CompatibilityLayer` and route lifecycle.
3. Keep default mapping behaviour equivalent for Hotline/SynHX.
4. Extend unit tests with `rstest` parameterized coverage.
5. Extend behavioural tests with `rstest-bdd` scenarios.
6. Update `docs/design.md` and `docs/users-guide.md`.
7. Run postgres setup and all gates with log capture.
8. Mark roadmap item 1.5.6 done in `docs/roadmap.md` with completion note.

## Progress

- [x] (2026-02-20 13:20Z) Reviewed roadmap 1.5.6 acceptance criteria,
  dependency on 1.5.5, ADR-002, and ADR-003.
- [x] (2026-02-20 13:20Z) Collected current routing and compatibility context
  from `src/wireframe/router.rs`, `src/wireframe/compat_layer.rs`,
  `src/wireframe/compat_policy.rs`, and existing behavioural suites.
- [x] (2026-02-20 13:20Z) Collected testing and verification requirements from
  project testing and verification guides.
- [x] (2026-02-20 13:20Z) Drafted this ExecPlan at the requested path.
- [x] (2026-02-20 16:49Z) Confirmed current implementation outcome: the
  guardrail path keeps request-side decode and login metadata capture in
  `CompatibilityLayer::on_request`, and keeps reply augmentation in
  `CompatibilityLayer::on_reply`; authentication execution remains in
  `src/login.rs::handle_login`.
- [x] (2026-02-20 16:49Z) Updated `docs/design.md` and `docs/users-guide.md`
  with roadmap 1.5.6 progress and preserved-behaviour notes.
- [x] (2026-02-20 16:49Z) Updated roadmap item 1.5.6 with in-progress status,
  captured outcomes, and explicit remaining scope.
- [x] Implementation started.
- [ ] Strategy/augmenter wiring merged and validated.
- [x] Docs updated for roadmap item 1.5.6.
- [ ] Roadmap item 1.5.6 marked done after implementation and gate evidence.

## Surprises & Discoveries

- `PLANS.md` is absent, so ExecPlan governance is driven by repository
  `AGENTS.md` and the standard ExecPlan structure.
- The current compatibility guardrail already centralizes reply augmentation
  (`on_reply`), but authentication remains in the login handler path; 1.5.6 is
  primarily an architectural split and wiring task.
- The explicit `AuthStrategy` and `LoginReplyAugmenter` contracts are not yet
  present in source modules, so roadmap 1.5.6 cannot be closed in this pass.
- Existing BDD coverage for login compatibility and guardrails provides a strong
  regression baseline for preserving default behaviour.
- `make typecheck` is available in this repository and should be included in
  verification gates.

## Decision Log

- Decision: treat 1.5.6 as ADR-003 phase 1 and phase 2 delivery (introduce
  interfaces + wire through guardrail entrypoint) while preserving current
  client-visible behaviour. Rationale: aligns with roadmap acceptance and keeps
  scope bounded. Date/Author: 2026-02-20 / Codex.
- Decision: keep compatibility-specific strategy/augmenter ownership in
  wireframe adapter modules and avoid domain-layer quirk leakage. Rationale:
  aligns with hexagonal boundary constraints and ADR-002/ADR-003 intent.
  Date/Author: 2026-02-20 / Codex.
- Decision: require both unit (`rstest`) and behavioural (`rstest-bdd`)
  coverage for happy, unhappy, and edge paths before closing roadmap 1.5.6.
  Rationale: explicitly required by task acceptance and repository testing
  guidance. Date/Author: 2026-02-20 / Codex.
- Decision: keep roadmap item 1.5.6 in-progress (not done) until explicit
  `AuthStrategy` and `LoginReplyAugmenter` wiring is present and validated.
  Rationale: acceptance criteria call for those contracts at the guardrail
  entrypoint, which is not yet fully represented in current source.
  Date/Author: 2026-02-20 / Codex.

## Outcomes & Retrospective

Implementation is underway and partially reflected in documentation.

Current outcomes:

- `docs/design.md` now records the 1.5.6 responsibility split in the guardrail
  path (request-side metadata capture versus reply-side augmentation).
- `docs/users-guide.md` now clarifies that the 1.5.6 refactor preserves default
  Hotline 1.8.5/1.9 and SynHX behaviour.
- `docs/roadmap.md` now tracks item 1.5.6 as in progress with explicit
  remaining scope: wire `AuthStrategy` and `LoginReplyAugmenter` contracts into
  the guardrail routing entrypoint.

Remaining completion evidence required:

- strategy/augmenter wiring merged in source and tests;
- verification gates run with captured logs;
- roadmap item 1.5.6 flipped to done with completion date.
