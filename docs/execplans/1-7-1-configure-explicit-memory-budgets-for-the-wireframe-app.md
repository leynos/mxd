# Configure explicit `MemoryBudgets` for the wireframe app (roadmap 1.7.1)

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: PLANNED

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 1.7.1 requires `mxd-wireframe-server` to configure explicit
`MemoryBudgets` for inbound buffering, using MXD's Hotline transaction and
streaming limits rather than the wireframe runtime's derived defaults. After
this work, the wireframe runtime must reject oversized fragmented inputs
predictably, disconnect stalled fragmented inputs on a documented timeout, and
preserve normal behaviour for valid clients that stay within the configured
limits.

This task is deliberately narrower than a protocol redesign. It must preserve
the current hexagonal boundary and the in-tree Hotline framing logic while
using wireframe v0.3.0's explicit budgeting facilities to make the runtime
limits visible, intentional, and test-backed.

Success is observable when:

- `src/server/wireframe/mod.rs` sets explicit per-message, per-connection, and
  in-flight `MemoryBudgets` instead of relying on derived defaults;
- the runtime also applies an explicit read timeout suitable for stalled
  fragmented inputs, and disconnect semantics are documented;
- unit tests built with `rstest` cover the budget derivation and unhappy-path
  invariants;
- behavioural and integration tests built on the 1.6.1 binary harness cover
  happy, unhappy, and edge cases for soft-pressure, hard-cap, and stall
  handling;
- `docs/design.md` records the budget formulas and transport trade-offs;
- `docs/users-guide.md` documents the user-visible effect of oversize and stall
  handling;
- `docs/roadmap.md` marks 1.7.1 done only after all quality gates pass.

## Constraints

- Preserve the current hexagonal boundary from `docs/design.md`: transport
  budgeting, disconnect policy, and timeout handling stay in wireframe adapter
  code, not in domain handlers.
- Keep `src/wireframe/codec/framed.rs` and
  `src/wireframe/codec/frame.rs` as the authority for Hotline's 20-byte header
  framing and multi-fragment reassembly.
- Keep `.fragmentation(None)` unless Stage A proves that explicit
  `MemoryBudgets` cannot satisfy acceptance criteria without enabling wireframe
  transport fragmentation. If that proof appears, stop and escalate, because it
  widens scope into roadmap items 1.7.2 and 1.7.3.
- Derive the explicit budgets from existing Hotline protocol and streaming
  limits (`MAX_FRAME_DATA`, `MAX_PAYLOAD_SIZE`, and the 1.3.2 streaming total
  limit), not from unexplained inline literals in the app builder.
- Keep new Rust modules under 400 lines. If budget derivation makes
  `src/server/wireframe/mod.rs` too large, extract a small helper module.
- Use `rstest` for unit coverage and `rstest-bdd` for behavioural coverage
  where it improves clarity. Reuse the 1.6.1 binary-under-test harness rather
  than inventing a second server bootstrap path.
- Use `pg_embedded_setup_unpriv` for local PostgreSQL-backed validation.
  The in-repo guide path is `docs/pg-embed-setup-unpriv-users-guide.md`; use
  that filename in docs and commands.
- Do not adopt `wireframe::testkit` or `wireframe_testing` as part of 1.7.1
  unless a tiny helper use is unavoidable. Roadmap item 1.7.2 owns the broader
  testkit migration.
- Update `docs/design.md` and `docs/users-guide.md` as part of the same
  feature, using en-GB-oxendict spelling and wrapped Markdown.
- Do not mark roadmap item 1.7.1 done until implementation, tests, lint,
  formatting, Markdown validation, Mermaid validation, and type checks all
  succeed.

## Tolerances (exception triggers)

- Scope: if the work grows beyond 12 files or roughly 500 net lines of code,
  stop and review whether the plan has leaked into 1.7.2 or 1.7.3 territory.
- Architecture: if satisfying the acceptance criteria requires public API
  changes outside `src/server/wireframe/`, `src/wireframe/`, `test-util/`, and
  documentation files, stop and escalate.
- Semantics: if explicit `MemoryBudgets` cannot be observed or enforced
  cleanly with the current codec path, stop and choose one of two explicit
  options:
  1. add a narrow adapter-side guard consistent with 1.7.1; or
  2. re-scope into a protocol/transport redesign tracked by a later roadmap
     item.
- Validation: if any new timing-sensitive test needs sleeps longer than one
  second or remains flaky after two tightening passes, redesign the fixture
  rather than widening the timeout budget.
- Dependencies: if a new crate or Cargo feature is required, stop and request
  approval before proceeding.

## Risks

- Risk: budget formulas that are too small disconnect valid large Hotline
  transactions or streaming requests. Severity: high. Likelihood: medium.
  Mitigation: derive limits from existing protocol constants, add
  exact-boundary tests, and document every formula in `docs/design.md`.
- Risk: a read timeout tuned for stall detection disconnects legitimate slow
  clients. Severity: high. Likelihood: medium. Mitigation: base the timeout on
  current test harness behaviour, cover a below-threshold happy path, and keep
  the timeout transport-scoped rather than user-configurable in this change.
- Risk: soft-pressure behaviour is difficult to assert directly without the
  1.7.2 testkit work. Severity: medium. Likelihood: medium. Mitigation: assert
  observable outcomes near the limit, keep hard-cap assertions direct, and
  defer deeper transport instrumentation to 1.7.2.
- Risk: the current Hotline codec buffers one logical fragmented transaction at
  a time, so per-connection and in-flight budget values may collapse
  operationally. Severity: medium. Likelihood: medium. Mitigation: record the
  present limitation explicitly and still set both caps intentionally.
- Risk: documentation drift between this feature and the existing wireframe
  migration notes. Severity: medium. Likelihood: low. Mitigation: update
  `docs/design.md`, `docs/users-guide.md`, and `docs/roadmap.md` together.

## Agent team and ownership

Implementation uses a coordinated agent team with explicit ownership:

- Architecture agent:
  derives the budget model, confirms the acceptable timeout semantics, and
  records the design decisions in `docs/design.md`.
- Runtime implementation agent:
  adds the explicit budget and timeout configuration in the wireframe bootstrap
  and extracts helper code where needed.
- Verification agent:
  owns new `rstest`, integration, and `rstest-bdd` coverage, plus the full
  validation sequence with captured logs.
- Documentation agent:
  updates `docs/users-guide.md`, records the completion note in
  `docs/roadmap.md`, and ensures the design document reflects the final
  implementation rather than the initial intention.

Handoff rule: each stage must leave file-level evidence and executable command
results before the next stage begins.

## Context and orientation

Current relevant implementation state:

- `src/server/wireframe/mod.rs` builds the app with
  `HotlineApp::default().fragmentation(None)` and currently does not call
  `.memory_budgets(...)` or `.read_timeout_ms(...)`.
- `src/wireframe/codec/frame.rs` reports
  `max_frame_length() == HEADER_LEN + MAX_FRAME_DATA`, anchoring the transport
  frame budget to the Hotline header and per-frame payload limit.
- `src/wireframe/codec/framed.rs` already enforces core protocol invariants for
  fragmented transactions:
  - invalid flags are rejected;
  - `total_size` above `MAX_PAYLOAD_SIZE` is rejected;
  - continuation header mismatches are rejected; and
  - incomplete reassembly at EOF fails with `UnexpectedEof`.
- `src/transaction/reader/streaming.rs` and
  `tests/transaction_streaming.rs` already exercise the 1.3.2 streaming path
  and its total-size guardrails. 1.7.1 must align transport buffering with
  those limits rather than replace them.
- `tests/common.rs` and `test-util/src/wireframe_bdd_world.rs` already provide
  the binary-under-test harness introduced by 1.6.1. Reuse that harness for
  socket-level integration tests and for any new behavioural scenarios.
- The wireframe user guide documents `MemoryBudgets`, soft-pressure, hard-cap
  enforcement, and read timeouts, but the server bootstrap has not yet adopted
  them explicitly.

Requirements and design anchors:

- `docs/roadmap.md` item 1.7.1 and dependencies 1.3.2, 1.6.1.
- `docs/design.md` for architectural scope and adapter boundaries.
- `docs/protocol.md` for Hotline transaction framing semantics.
- `docs/verification-strategy.md` for test and verification obligations.
- `docs/rust-testing-with-rstest-fixtures.md` and
  `docs/rstest-bdd-users-guide.md` for fixture and behavioural test style.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` for deterministic
  test design.
- `docs/rust-doctest-dry-guide.md` for keeping any new examples/test docs
  minimal and accurate.
- `docs/wireframe-users-guide.md` and
  `docs/wireframe-v0-2-0-to-v0-3-0-migration-guide.md` for wireframe runtime
  semantics and budgeting rules.
- `docs/ortho-config-users-guide.md` and `docs/users-guide.md` for the operator
  and CLI surface.
- `docs/pg-embed-setup-unpriv-users-guide.md` for PostgreSQL test setup.

## Plan of work

### Stage A: design the budget model and disconnect contract

- Inventory the current protocol and transport limits:
  - `MAX_FRAME_DATA` for one Hotline frame payload;
  - `MAX_PAYLOAD_SIZE` for one reassembled Hotline transaction;
  - the 1.3.2 streaming total-size cap; and
  - any existing socket read expectations in the 1.6.1 integration harness.
- Decide and document exact formulas for:
  - `bytes_per_message`;
  - `bytes_per_connection`; and
  - `bytes_in_flight`.
- Decide whether the per-connection and in-flight caps are intentionally equal
  in the current implementation, or whether they should differ to encode future
  intent even if present behaviour is equivalent.
- Decide the explicit read timeout used to disconnect stalled fragmented
  inputs. The timeout must be strong enough to make the acceptance criteria
  testable and modest enough not to penalize normal test traffic.
- Record the design decisions in `docs/design.md`, including why 1.7.1 keeps
  `.fragmentation(None)` and does not adopt `wireframe::testkit`.

Exit criteria:

- every budget value has a documented source and rationale;
- the disconnect contract for oversize and stall cases is explicit; and
- the design document is ready to describe the final code change.

### Stage B: implement explicit runtime configuration

- Introduce a small helper for budget derivation if inline builder code becomes
  unclear. Candidate locations:
  - `src/server/wireframe/mod.rs`; or
  - a new `src/server/wireframe/budgets.rs` module if needed for clarity.
- Configure explicit `MemoryBudgets` in the app builder using the Stage A
  formulas.
- Configure `read_timeout_ms(...)` explicitly in the same builder so stalled
  fragmented inputs are disconnected predictably.
- Keep `.fragmentation(None)` unless Stage A raised an approved scope change.
- Ensure startup validation still exercises the app builder path so broken
  budget configuration fails during bootstrap instead of at runtime.
- Keep routing, middleware ordering, XOR compatibility, and handshake
  behaviour unchanged apart from the intended memory/timeout policy.

Exit criteria:

- `mxd-wireframe-server` configures explicit budgets and timeout values in
  code;
- no unrelated protocol behaviour changes; and
- the builder remains small, readable, and locally testable.

### Stage C: add unit coverage with `rstest`

Add parameterized unit tests for the new budget and timeout logic:

- exact budget derivation from the chosen protocol constants;
- boundary invariants such as `bytes_per_message <= min(connection, in_flight)`
  where required by the chosen formulas;
- arithmetic safety and non-zero conversions for `BudgetBytes`;
- timeout configuration derivation, if represented by a helper;
- builder-focused tests in `src/server/wireframe/tests.rs` or a new adjacent
  test module.

Use `#[rstest]` parameterization instead of duplicating cases. Where wireframe
does not expose a direct getter for configured budgets, test the derivation
helper directly and rely on Stage D for end-to-end behavioural evidence.

Exit criteria:

- unit tests lock down the formulas and edge cases;
- unhappy-path arithmetic or conversion failures are covered; and
- the tests remain deterministic without mutating global state.

### Stage D: add integration and behavioural coverage

Add integration tests against the real `mxd-wireframe-server` binary, using the
1.6.1 harness and raw socket control where necessary.

Target scenarios:

- Happy path / soft-pressure:
  send a fragmented input that stays within the configured hard caps and prove
  the server continues to make progress instead of disconnecting. Prefer an
  observable outcome such as a successful login or a valid error reply after
  frame completion.
- Hard cap:
  send a fragmented input whose declared or buffered size exceeds the explicit
  budget and assert a predictable disconnect or transport failure.
- Stalled fragment:
  send an initial fragment, stop sending before completion, wait past the
  configured timeout, and assert that the connection is closed predictably.
- Boundary:
  prove that an exact-limit case is accepted and the smallest exceeding case is
  rejected.

Testing split:

- Use plain integration tests for low-level socket timing and raw fragmented
  byte control.
- Add `rstest-bdd` scenarios where the disconnect semantics can be expressed
  clearly without hiding the transport conditions.
- Reuse existing test helpers and `test-util` fixtures. Do not broaden this
  stage into a `wireframe::testkit` migration; roadmap item 1.7.2 owns that
  refactor.

PostgreSQL validation:

- Pre-login oversize and stall tests should prefer the lightest backend
  requirements possible.
- At least one full validation pass must still run after
  `pg_embedded_setup_unpriv` so the PostgreSQL-backed matrix remains covered.

Exit criteria:

- happy, unhappy, and edge cases are covered;
- at least one behavioural scenario uses `rstest-bdd` where it adds clarity;
  and
- the acceptance criteria around soft-pressure, hard-cap, and stalled inputs
  are proven by automated tests.

### Stage E: update documentation and roadmap state

- Update `docs/design.md` with:
  - the chosen budget formulas;
  - the timeout decision for stalled fragmented inputs;
  - the reason `.fragmentation(None)` remains in place; and
  - any limitation where per-connection and in-flight caps currently collapse
    operationally.
- Update `docs/users-guide.md` so operators know that the wireframe listener
  now applies explicit inbound memory budgets and disconnects oversize or
  stalled fragmented traffic predictably.
- Update `docs/roadmap.md` only after all gates pass, marking 1.7.1 done with
  a concise completion summary and date.

Exit criteria:

- design and operator documentation match the implemented behaviour; and
- the roadmap reflects the finished state only after evidence exists.

### Stage F: verification and quality gates

Run the full validation sequence with `tee` and `set -o pipefail` so failures
are preserved. Use branch-safe log filenames.

Recommended sequence:

```sh
set -o pipefail
PROJECT=$(basename "$PWD")
BRANCH=$(git branch --show-current | tr '/ ' '__')

pg_embedded_setup_unpriv \
  | tee "/tmp/pg-setup-${PROJECT}-${BRANCH}.log"

make fmt \
  | tee "/tmp/fmt-${PROJECT}-${BRANCH}.log"

make check-fmt \
  | tee "/tmp/check-fmt-${PROJECT}-${BRANCH}.log"

make lint \
  | tee "/tmp/lint-${PROJECT}-${BRANCH}.log"

make test \
  | tee "/tmp/test-${PROJECT}-${BRANCH}.log"

make markdownlint \
  | tee "/tmp/markdownlint-${PROJECT}-${BRANCH}.log"

make nixie \
  | tee "/tmp/nixie-${PROJECT}-${BRANCH}.log"

make typecheck \
  | tee "/tmp/typecheck-${PROJECT}-${BRANCH}.log"
```

If any command fails, fix the issue and rerun the affected gate before marking
the roadmap entry complete.

## Concrete implementation checklist

1. Add a documented budget-derivation helper or keep the logic local if it
   stays small and obvious.
2. Configure explicit `MemoryBudgets` and an explicit read timeout in
   `src/server/wireframe/mod.rs`.
3. Add `rstest` unit coverage for formulas and edge cases.
4. Add raw-socket integration coverage for hard-cap and stall disconnects.
5. Add `rstest-bdd` coverage where the behaviour reads clearly as scenarios.
6. Update `docs/design.md` with the final design decisions.
7. Update `docs/users-guide.md` with operator-visible behaviour.
8. Run all gates in Stage F and capture logs.
9. Mark roadmap item 1.7.1 done only after all gates pass.

## Progress

- [x] (2026-04-10) Reviewed roadmap item 1.7.1, the existing wireframe runtime,
  and the referenced design, protocol, testing, verification, and user-guide
  documents.
- [x] (2026-04-10) Confirmed that `src/server/wireframe/mod.rs` currently sets
  `.fragmentation(None)` but does not configure explicit `MemoryBudgets` or a
  runtime read timeout.
- [x] (2026-04-10) Confirmed that the 1.6.1 binary-under-test harness and the
  current raw-socket integration suites are the right starting point for 1.7.1
  coverage.
- [x] (2026-04-10) Confirmed that the repository's PostgreSQL setup guide is
  `docs/pg-embed-setup-unpriv-users-guide.md`, which supersedes the prompt's
  `pg-embedded-...` spelling for in-repo references.

## Surprises & Discoveries

- Observation: the runtime already depends on protocol-level limits in the
  Hotline codec (`MAX_FRAME_DATA`, `MAX_PAYLOAD_SIZE`) but still relies on
  wireframe's derived budgeting defaults. Impact: 1.7.1 should align those two
  layers explicitly instead of inventing new limits.
- Observation: the current builder sets neither `.memory_budgets(...)` nor
  `.read_timeout_ms(...)`. Impact: stalled fragmented-input handling needs an
  explicit timeout decision as part of this feature, not just new byte caps.
- Observation: roadmap item 1.7.2 explicitly owns the migration to
  `wireframe::testkit` helpers. Impact: 1.7.1 should keep its tests narrow and
  evidence-focused rather than bundling in a test harness rewrite.
- Observation: the prompt named
  `docs/pg-embedded-setup-unpriv-users-guide.md`, but the repository's actual
  file is `docs/pg-embed-setup-unpriv-users-guide.md`. Impact: follow the
  in-repo filename in the plan and in subsequent documentation changes.

## Decision Log

- Decision: keep Hotline-specific fragmentation in the in-tree codec modules
  for 1.7.1. Rationale: this roadmap item is about explicit budgeting and
  predictable disconnects, not about replacing the current framing model with
  wireframe transport fragmentation. Date/Author: 2026-04-10 / Assistant
- Decision: treat stalled-input handling as an explicit transport timeout plus
  predictable connection closure. Rationale: that matches the acceptance
  criteria and can be tested cleanly without inventing a new protocol reply
  surface. Date/Author: 2026-04-10 / Assistant
- Decision: defer broad `wireframe::testkit` adoption to roadmap item 1.7.2.
  Rationale: the roadmap already separates explicit budgeting from testkit
  migration, and current harnesses are sufficient for 1.7.1 acceptance.
  Date/Author: 2026-04-10 / Assistant
- Decision: use the repository's
  `docs/pg-embed-setup-unpriv-users-guide.md` path as the authoritative guide
  reference. Rationale: the file exists in-tree and is the document that future
  contributors will actually open. Date/Author: 2026-04-10 / Assistant

## Outcomes & Retrospective

Pending implementation.
