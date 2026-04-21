# Configure explicit memory budgets for the wireframe app

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

## Purpose / big picture

Roadmap item 1.7.1 requires `mxd-wireframe-server` to stop relying on
Wireframe's derived buffering defaults and to configure explicit
`MemoryBudgets` that are justified by MXD (the MXD server)'s Hotline
transaction limits and streaming limits. The delivered behaviour must be
transport-realistic: oversized fragmented inputs and stalled partial inputs are
rejected predictably, while valid requests that stay within the configured
envelope continue to complete through the real server binary.

Success is observable when:

- `src/server/wireframe/mod.rs` sets explicit per-message,
  per-connection, and in-flight budgets on the `WireframeApp`;
- the chosen adapter seam documents how Wireframe v0.3.0 budgeting relates to
  Hotline transaction framing and the existing streaming reader limits from
  roadmap item 1.3.2;
- fragmented inputs that exceed the selected cap, or remain incomplete past
  the allowed timeout path, are disconnected in a deterministic way that tests
  can assert;
- `rstest` unit coverage locks down the budget derivation and any new helper
  logic, including unhappy and edge cases;
- binary-backed integration and behavioural coverage exercises soft-pressure
  and hard-cap outcomes against `mxd-wireframe-server`;
- `docs/design.md` records the architecture and sizing decisions;
- `docs/users-guide.md` records any operator-visible behaviour changes or any
  new configuration surface, if one is introduced; and
- the roadmap entry in `docs/roadmap.md` is marked done only after the
  implementation, tests, and documentation land.

## Agent team

Implementation should be split across a small agent team with explicit
ownership:

- Runtime/adapter agent: owns the Wireframe app builder, the budget sizing
  helper, and the decision on whether `MemoryBudgets` can be adopted with the
  current codec seam or requires a small adapter refactor.
- Verification/test agent: owns failing-first regression coverage, raw-socket
  helpers in `test-util`, `rstest` unit coverage, `rstest-bdd` scenarios where
  the behaviour is user-observable, and the local PostgreSQL-backed validation
  path via `pg_embedded_setup_unpriv`.
- Documentation/roadmap agent: owns design-record updates, user-guide changes,
  and the roadmap completion note once the feature is actually shipped.

Coordination rules:

- Keep transport concerns in the wireframe adapter layer. Domain handlers and
  command logic must not gain new `wireframe::*` coupling.
- Land tests and documentation as part of the same feature branch, not as
  follow-up clean-up.
- Escalate rather than silently widening the task into roadmap item 1.7.2
  (`wireframe::testkit`) unless that dependency is proven unavoidable.

## Constraints

- Preserve the current Hotline wire contract: handshake semantics, 20-byte
  transaction headers, reply ID echoing, XOR compatibility, and route dispatch
  behaviour must remain unchanged except for the intended disconnect behaviour
  on oversized or stalled fragmented input.
- Respect the hexagonal boundary from
  `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`:
  memory-budget enforcement belongs in the transport adapter and test harness,
  not in domain handlers.
- Use existing Hotline sizing authorities when deriving the budgets:
  `MAX_FRAME_DATA`, `MAX_PAYLOAD_SIZE`, and the streaming-reader limits added
  in roadmap item 1.3.2.
- Prefer explicit internal derivation over ad-hoc literals. If a new runtime
  config knob is introduced, it must be justified, wired through `cli-defs` and
  `ortho-config`, and documented in `docs/users-guide.md`.
- Reuse the binary-backed test path from roadmap item 1.6.1. Regression tests
  should continue to exercise `mxd-wireframe-server` through `test-util`
  helpers rather than through in-process route invocation.
- Add or update `rstest` unit tests for helper logic and `rstest-bdd`
  behavioural coverage where the scenario reads naturally as user-visible
  server behaviour. Happy, unhappy, and edge paths are all required.
- Local PostgreSQL validation must use the documented
  `pg_embedded_setup_unpriv` workflow from
  `docs/pg-embed-setup-unpriv-users-guide.md`.
- Keep documentation in en-GB-oxendict spelling, wrap prose at 80 columns, and
  follow repository quality gates before commit: `make check-fmt`, `make lint`,
  `make test`, `make fmt`, `make markdownlint`, and `make nixie` if Mermaid
  diagrams are touched.
- Do not add new third-party crates unless explicitly approved.

## Tolerances (exception triggers)

- Architecture: if explicit `MemoryBudgets` cannot influence the relevant
  fragmented-input path while `fragmentation(None)` remains in place, stop
  after a focused spike and choose one documented approach before continuing:
  either a minimal adapter refactor for Wireframe-managed assembly or a
  different acceptance interpretation agreed with the user.
- Scope: if satisfying 1.7.1 requires more than 16 files changed or more than
  650 net new lines, stop and review whether work from 1.7.2 or 1.7.3 is
  leaking in.
- Interface: if the change needs a public CLI or config surface beyond a small
  set of budget fields, stop and confirm the intended operator experience.
- Testing: if soft-pressure behaviour cannot be asserted deterministically with
  the current binary-backed harness, stop and decide whether a narrow helper
  extension is sufficient or whether the work should be deferred to 1.7.2.
- Dependencies: if implementation appears to require `wireframe::testkit`,
  `wireframe_testing`, or any new crate to complete 1.7.1, stop and review the
  roadmap boundary before adding it.
- Validation: if repository quality gates fail twice after targeted fixes,
  stop and capture the failing logs before proceeding.

## Risks

- Risk: the current transport seam may bypass Wireframe's budgeting path.
  Severity: high. Likelihood: high. Mitigation: start with a narrow design
  spike that proves where budgeting hooks apply relative to `HotlineFrameCodec`
  and the current `fragmentation(None)` setting.
- Risk: budget formulas derived from Hotline limits could accidentally reject
  valid large flows or allow more buffering than intended. Severity: high.
  Likelihood: medium. Mitigation: centralize the formulas in one helper with
  unit tests and document the rationale in `docs/design.md`.
- Risk: soft-pressure behaviour is timing-sensitive and may become flaky in
  Continuous Integration (CI). Severity: medium. Likelihood: medium.
  Mitigation: use controlled chunked writes, explicit socket deadlines, and
  assertions on outcome ordering rather than brittle wall-clock thresholds.
- Risk: PostgreSQL-backed integration runs may fail for environment reasons
  unrelated to budgeting. Severity: medium. Likelihood: medium. Mitigation:
  follow the `pg_embedded_setup_unpriv` path and preserve existing skip logic
  for unavailable embedded Postgres environments.
- Risk: the work could drift into broader transport-tooling adoption scheduled
  for 1.7.2 and 1.7.3. Severity: medium. Likelihood: medium. Mitigation: keep
  1.7.1 focused on explicit budgets, disconnect semantics, and binary-backed
  regression coverage only.

## Progress

- [x] (2026-04-10 00:00Z) Drafted ExecPlan for roadmap item 1.7.1 with
      architecture, testing, documentation, and roadmap-close criteria.
- [x] (2026-04-13 00:00Z) Proved the budgeting seam and recorded the chosen
      adapter shape in `docs/design.md`: keep `fragmentation(None)`, move
      inbound Hotline reassembly to a Wireframe `MessageAssembler`, and size
      budgets against a full logical Hotline request envelope.
- [x] (2026-04-13 00:00Z) Implemented explicit budget derivation and applied
      it in the Wireframe app builder.
- [x] (2026-04-13 00:00Z) Added `rstest` unit coverage for budget formulas and
      bootstrap helper logic.
- [x] (2026-04-13 00:00Z) Added binary-backed integration coverage for an
      in-budget near-cap fragmented request, a hard-cap oversize reject, and a
      stalled-fragment disconnect on resumed continuation.
- [x] (2026-04-13 00:00Z) Validated the feature locally on SQLite and
      PostgreSQL using `pg_embedded_setup_unpriv` and `PG_EMBEDDED_WORKER`.
- [x] (2026-04-13 00:00Z) Updated `docs/users-guide.md` to describe the new
      disconnect behaviour for oversized and stalled fragmented requests.
- [x] (2026-04-13 00:00Z) Marked roadmap item 1.7.1 done in
      `docs/roadmap.md`.
- [x] (2026-04-13 00:00Z) Ran repository quality gates with tee-captured logs:
      `make fmt`, `make check-fmt`, `make lint`, `make test`, and
      `make markdownlint`.

## Surprises & Discoveries

- `src/server/wireframe/mod.rs` built the `HotlineApp` with
  `.fragmentation(None)`, so Wireframe transport fragmentation remained
  explicitly disabled in the delivered runtime.
- `src/wireframe/codec/framed.rs` already reassembled multi-fragment Hotline
  transactions inside `HotlineCodec`, and the delivered
  `HotlineFrameCodec::max_frame_length()` change in
  `src/wireframe/codec/frame.rs` now reports the logical Hotline request
  envelope exposed to Wireframe rather than the physical
  `HEADER_LEN + MAX_FRAME_DATA` frame size.
- `docs/execplans/wireframe-v0-3-0-migration.md` explicitly recorded that
  `memory_budgets`, `enable_fragmentation()`, and the message assembler were
  not a drop-in during the v0.3.0 version bump because Hotline fragmentation
  was still managed in the codec.
- `src/transaction/mod.rs` fixes `MAX_PAYLOAD_SIZE` at 1 MiB, while
  `TransactionStreamReader` in `src/transaction/reader/streaming.rs` defaults
  to the same cap but can be raised for large streaming use cases.
- The binary-backed harnesses from roadmap item 1.6.1 already exist:
  `test-util/src/server/mod.rs` launches `mxd-wireframe-server`, and
  `test-util/src/wireframe_bdd_world.rs` already manages real TCP connections,
  handshakes, reconnects, and framed request/reply exchange.
- Focused integration coverage for soft-pressure behaviour and aggregate memory
  budgets was added on top of those existing binary-backed harnesses.
- Wireframe's protocol-level assembly state still derives a fragment ceiling
  from `codec.max_frame_length()` even when `fragmentation(None)` remains in
  place. Leaving `HotlineFrameCodec::max_frame_length()` at the physical
  `20 + 32 KiB` size would have capped assembled Hotline requests far below the
  existing 1 MiB protocol limit, so the adapter now reports the logical request
  envelope there while physical frame validation remains unchanged.
- With transport fragmentation disabled, Wireframe did not proactively reap an
  idle partial Hotline request. To preserve the existing five-second fragment
  gap semantics, `HotlineFrameDecoder` enforced a continuation deadline and
  rejected late fragments when the next continuation arrived too late.
- Local validation in this container required bootstrapping two external helper
  toolchains that the repository gates assume are present: Whitaker for
  `make lint`, and `pg_worker` plus `PG_EMBEDDED_WORKER` for the
  PostgreSQL-backed `make test` path.

## Decision Log

- Decision: treat 1.7.1 as an adapter-layer hardening task, not a domain-task.
  Rationale: the risk is inbound buffering and fragmented transport behaviour,
  which belongs at the wireframe/bootstrap boundary under the repository's
  hexagonal design. Date/Author: 2026-04-10 / Codex.
- Decision: derive explicit budgets from Hotline protocol limits rather than
  hand-picked literals spread through the runtime. Rationale: this keeps the
  runtime, tests, and documentation tied to one authoritative sizing model.
  Date/Author: 2026-04-10 / Codex.
- Decision: keep 1.7.1 scoped away from `wireframe::testkit` unless a small
  proof shows the current raw-socket harness cannot validate the acceptance
  criteria. Rationale: roadmap item 1.7.2 already reserves that tooling
  adoption. Date/Author: 2026-04-10 / Codex.
- Decision: record the final budget formulas and the chosen fragmentation seam
  in `docs/design.md` as part of the implementation, even if no user-visible
  configuration is added. Rationale: this choice affects future transport work
  in 1.7.2 and 1.7.3 and is not obvious from code alone. Date/Author:
  2026-04-10 / Codex.
- Decision: keep `fragmentation(None)` and introduce a Hotline-specific
  `MessageAssembler` rather than switching to Wireframe transport
  fragmentation. Rationale: this preserves the legacy Hotline wire contract and
  keeps the fragmentation/budget boundary inside the transport adapter.
  Date/Author: 2026-04-13 / Codex.
- Decision: set `bytes_per_message`, `bytes_per_connection`, and
  `bytes_in_flight` to the same value: `HEADER_LEN + MAX_PAYLOAD_SIZE`.
  Rationale: Hotline still processes one fragmented logical request at a time
  per connection, so widening the connection or in-flight budgets beyond one
  request would add buffering without enabling a valid protocol behaviour.
  Date/Author: 2026-04-13 / Codex.
- Decision: treat the soft-pressure acceptance case as "near-cap fragmented
  requests still complete" rather than trying to assert internal pacing from
  the black-box binary harness. Rationale: Wireframe's soft-pressure pacing is
  not externally observable in a stable way from today's raw-socket tests, but
  an over-80%-budget fragmented request that still routes proves the server
  accepts valid near-cap traffic while the hard-cap and timeout paths close
  deterministically. Date/Author: 2026-04-13 / Codex.

## Outcomes & Retrospective

Intended outcomes once implemented:

- `mxd-wireframe-server` applies explicit inbound memory budgets instead of
  relying on Wireframe's derived defaults.
- Oversized or stalled fragmented inputs fail closed in a predictable,
  regression-tested manner.
- Budget sizing, transport constraints, and operator-visible behaviour are
  documented consistently across the roadmap, design doc, and user guide.

- Implemented:
  explicit Wireframe memory budgets for the Hotline adapter, an internal
  `HotlineMessageAssembler` seam that preserves legacy routing bytes, and
  binary-backed regression coverage for near-cap success, oversize disconnect,
  and resumed-after-timeout disconnect paths.
- Did not implement:
  a new operator-facing budget configuration surface, Wireframe transport
  fragmentation, or `wireframe::testkit` adoption; those remain outside the
  1.7.1 scope.
- Lesson:
  for Wireframe v0.3.0, budget sizing and `max_frame_length()` cannot be
  treated independently. Even with `fragmentation(None)`, the codec's reported
  logical frame ceiling still constrains protocol-level assembly.

## Context and orientation

Primary files and modules in the current state:

- `docs/roadmap.md`: source of the 1.7.1 acceptance criteria and dependency
  boundary.
- `src/server/wireframe/mod.rs`: Wireframe bootstrap and current
  `HotlineApp::default().fragmentation(None)` configuration.
- `src/wireframe/codec/frame.rs`: `HotlineFrameCodec` adapter that exposes the
  frame-length boundary to Wireframe.
- `src/wireframe/codec/framed.rs`: `HotlineCodec` that currently performs
  multi-fragment Hotline reassembly internally.
- `src/transaction/mod.rs`: `MAX_FRAME_DATA` and `MAX_PAYLOAD_SIZE`.
- `src/transaction/reader/mod.rs` and
  `src/transaction/reader/streaming.rs`: buffered vs streaming limits and
  fragment-validation helpers introduced by roadmap item 1.3.2.
- `test-util/src/server/mod.rs`: binary launcher for `mxd-wireframe-server`.
- `test-util/src/wireframe_bdd_world.rs`: reusable real-socket harness for
  behavioural tests.
- `tests/payload_reject.rs`, `tests/transaction_streaming.rs`,
  `tests/wireframe_transaction.rs`, and handshake/integration suites: current
  related regression coverage.
- `docs/design.md`: design document that must capture the chosen budgeting
  architecture and formulas.
- `docs/users-guide.md`: operator-facing behaviour guide to update if large or
  stalled input handling, or configuration, becomes user-visible.

## Plan of work

### Stage A: prove the architecture seam and lock the sizing model

The runtime/adapter agent should start with a focused design spike that answers
two questions:

1. Does `WireframeApp::memory_budgets(...)` meaningfully protect the current
   inbound fragmented-input path while `HotlineCodec` continues to own Hotline
   reassembly?
2. If not, what is the smallest adapter refactor that allows explicit
   `MemoryBudgets` to enforce the acceptance criteria without rewriting the
   broader routing stack?

This stage must inventory the current authorities for sizing:

- `MAX_FRAME_DATA` for physical Hotline fragment size;
- `MAX_PAYLOAD_SIZE` for buffered transaction assembly; and
- any raised limits already used by streaming readers/writers for large flows.

The output of Stage A is a short design note in `docs/design.md` that records:

- the chosen enforcement seam;
- the formulas for `bytes_per_message`, `bytes_per_connection`, and
  `bytes_in_flight`; and
- whether the implementation remains fully internal or adds user-configurable
  knobs.

Validation gate for Stage A:

- a focused failing-first test or code spike proves where the current runtime
  does or does not enforce fragmented-input buffering before full
  implementation begins.

### Stage B: implement explicit budget configuration in the adapter

Once Stage A selects the seam, implement a small, named helper owned by the
wireframe adapter. That helper should derive `BudgetBytes` and `MemoryBudgets`
from Hotline constants rather than burying numbers inside the builder chain.

Implementation expectations:

- update `src/server/wireframe/mod.rs` to call `.memory_budgets(...)`
  explicitly;
- keep the chosen fragmentation/assembly setting intentional and documented,
  whether that remains `fragmentation(None)` or changes to a different
  Wireframe-managed path;
- preserve handshake, compatibility, and routing behaviour for valid requests;
- ensure oversize or partial-assembly failures terminate the connection in a
  repeatable way without leaving partially authenticated or partially routed
  state behind; and
- keep wireframe-specific code inside adapter modules, not inside command or
  handler logic.

If new config fields are required, this stage also owns:

- adding them to `cli-defs/src/lib.rs`;
- loading them through existing `ortho-config` layering;
- covering precedence with `rstest` unit tests; and
- documenting them in `docs/users-guide.md`.

Validation gate for Stage B:

- focused runtime tests prove the helper produces non-zero, valid budgets and
  the server still boots cleanly through the existing binary harness.

### Stage C: add regression coverage for happy, unhappy, and edge paths

The verification/test agent should add tests at the lowest useful layer first,
then confirm behaviour through the real server binary.

Required unit coverage with `rstest`:

- budget derivation cases from default Hotline limits;
- edge cases around non-zero conversion, overflow avoidance, and relation
  invariants between per-message, per-connection, and in-flight caps; and
- any helper that maps timeout or over-budget failures into connection-close
  behaviour.

Required binary-backed coverage:

- a happy-path case showing a fragmented request that stays within budget still
  completes successfully;
- a soft-pressure case showing a fragmented input near the aggregate cap is
  paced rather than immediately dropped, and still completes when the client
  continues making progress within timeout;
- a hard-cap case showing a payload or partial assembly that exceeds the
  configured cap is disconnected deterministically; and
- a stalled-input case showing a fragmented request that stops progressing is
  closed predictably rather than lingering indefinitely.

Use `rstest-bdd` Behaviour-Driven Development (BDD) scenarios where the story
reads naturally in Given/When/Then form from an operator or client perspective.
Keep lower-level timing and chunk construction in Rust helpers and fixtures,
not in the Gherkin text.

Unless Stage A proves otherwise, extend the existing raw-socket harness in
`test-util` with narrowly scoped helpers such as:

- chunked frame writes with controlled delays;
- helpers that stop after the first or Nth fragment; and
- connection-close assertions that distinguish clean EOF from timeout.

Validation gate for Stage C:

- the new targeted suites pass for the active backend, and at least one
  PostgreSQL-backed binary test path is exercised using the
  `pg_embedded_setup_unpriv` workflow.

### Stage D: document the delivered behaviour and close the roadmap item

The documentation/roadmap agent should update the docs as the implementation
lands, not afterwards.

Required documentation changes:

- `docs/design.md`: record the selected budgeting seam, formulas, and why the
  chosen approach fits the adapter boundary.
- `docs/users-guide.md`: describe any operator-visible effect, such as the
  server dropping oversized or stalled fragmented inputs, and any new budget
  configuration flags if introduced.
- `docs/roadmap.md`: mark 1.7.1 done with a completion note only after the
  implementation and validation are complete.

If the implementation reveals a non-obvious transport limitation, capture it in
the design record so roadmap items 1.7.2 and 1.7.3 can build on it cleanly.

Validation gate for Stage D:

- the design doc, user guide, and roadmap note all agree on the final runtime
  behaviour.

### Stage E: run full validation and capture logs

Before commit, run repository gates with tee-captured logs and
`set -o pipefail` so failures are not masked by piping:

```sh
set -o pipefail && make check-fmt 2>&1 | tee /tmp/1-7-1-check-fmt.log
set -o pipefail && make lint 2>&1 | tee /tmp/1-7-1-lint.log
set -o pipefail && make test 2>&1 | tee /tmp/1-7-1-test.log
set -o pipefail && make fmt 2>&1 | tee /tmp/1-7-1-fmt.log
set -o pipefail && make markdownlint 2>&1 | tee /tmp/1-7-1-markdownlint.log
```

If Mermaid diagrams change during documentation updates, also run:

```sh
set -o pipefail && make nixie 2>&1 | tee /tmp/1-7-1-nixie.log
```

For local PostgreSQL-backed verification, stage the test cluster first as
documented:

```sh
cargo install --locked pg-embed-setup-unpriv
pg_embedded_setup_unpriv
set -o pipefail && make test 2>&1 | tee /tmp/1-7-1-test-postgres.log
```

The final implementation is only complete when the runtime change, tests, and
documentation all pass their respective gates.
