# Provide outbound transport and messaging traits

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md does not exist in this repository.

## Purpose / Big Picture

Deliver outbound transport and messaging traits so domain logic can emit
responses and notifications without depending on `wireframe` types. The
wireframe adapter will implement these traits, preserving the hexagonal
boundary described in `docs/design.md` and
`docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`.
Success is visible when domain modules only depend on the new traits, the
wireframe server still runs, and unit plus behaviour-driven development (BDD)
tests cover both success and failure paths for outbound delivery.

## Constraints

- Domain modules must not import `wireframe::*`; only adapter-layer modules
  (under `src/wireframe` and `src/server/wireframe.rs`) may depend on it.
- Avoid new external dependencies. If a new crate is required, stop and
  escalate.
- All new public APIs require Rustdoc comments with examples.
- Every new module must start with a module-level `//!` comment.
- Enforce formatting, lint, and test gates (`make check-fmt`, `make lint`,
  `make test`), plus Markdown checks when docs change (`make markdownlint`,
  `make nixie`, and `make fmt`).
- Documentation must use en-GB-oxendict spelling, wrapped at 80 columns.

## Tolerances (Exception Triggers)

- Scope: if more than 12 files change or net changes exceed 500 lines of code
  (LOC), stop and escalate.
- Interface: if a public API outside `src/server` or `src/wireframe` must
  change, stop and escalate.
- Dependencies: if a new external dependency is required, stop and escalate.
- Iterations: if tests or lint still fail after two fix attempts, stop and
  escalate.
- Ambiguity: if wireframe push/session APIs do not match assumptions and
  multiple interpretations exist, stop and present options with trade-offs.

## Risks

- Risk: wireframe push/session APIs may not align with the planned trait
  boundaries. Severity: medium Likelihood: medium Mitigation: inspect the
  wireframe crate API and align trait signatures to actual capabilities before
  integrating.

- Risk: outbound traits could force broad signature changes in domain code.
  Severity: medium Likelihood: low Mitigation: introduce a small, explicit
  outbound context struct and update only the route boundary
  (`process_transaction_bytes`) to pass it.

- Risk: tests that rely on Postgres may be flaky without the embedded setup.
  Severity: low Likelihood: medium Mitigation: use
  `pg_embedded_setup_unpriv::test_support::test_cluster` in tests that touch
  the database and record failure handling in test steps.

## Progress

    - [x] (2026-01-17 02:03Z) Capture current outbound flows, wireframe push
      APIs, and identify the best insertion point for the outbound traits.
    - [x] (2026-01-17 02:21Z) Define outbound traits and error types alongside
      the server boundary, with unit tests and test doubles.
    - [x] (2026-01-17 02:21Z) Implement wireframe adapters and thread outbound
      context into the route boundary and handlers.
    - [x] (2026-01-17 02:21Z) Add BDD coverage for outbound delivery behaviour
      and update documentation/roadmap entries.
    - [x] (2026-01-17 02:52Z) Run formatting, lint, test, and documentation
      checks; capture results.

## Surprises & Discoveries

- Observation: Wireframe's `ConnectionContext` is empty and does not expose
  peer or connection identifiers, so outbound messaging cannot rely on it.
  Evidence: `wireframe-0.2.0/src/hooks.rs` defines `ConnectionContext` as an
  empty struct. Impact: Outbound messaging must store push handles in
  per-connection protocol state or a registry keyed by a locally generated
  identifier.

## Decision Log

- Decision: Store wireframe push handles in per-connection protocol state and
  register them in a shared registry keyed by `OutboundConnectionId`.
  Rationale: Wireframe's `ConnectionContext` is empty, so protocol state is the
  only per-connection hook available for wiring outbound messaging without
  leaking wireframe types into domain code. Date/Author: 2026-01-17 02:21Z /
  Codex

## Outcomes & Retrospective

- Completed on 2026-01-17. Outbound adapters, routing updates, and new unit
  plus BDD coverage landed alongside refreshed design notes. Formatting, lint,
  test, and documentation gates all passed.

## Context and Orientation

Relevant areas to review before editing:

- `src/wireframe/protocol.rs` stores the wireframe lifecycle hooks where
  `PushHandle` can be captured on connection setup.
- `src/server/wireframe.rs` constructs the wireframe app and installs the
  routing middleware that will need outbound context injection.
- `src/wireframe/routes/mod.rs` hosts the routing entry point
  (`process_transaction_bytes`) that invokes `Command::process_with_outbound`.
- `src/handler.rs`, `src/commands/mod.rs`, and `src/commands/handlers.rs`
  define domain processing and the `Transaction` reply path.
- `docs/design.md` and
  `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`
  define the hexagonal boundaries that outbound ports must respect.
- `docs/wireframe-users-guide.md` documents push queues and session registry
  behaviour.
- Test guidance is in `docs/rust-testing-with-rstest-fixtures.md`,
  `docs/rstest-bdd-users-guide.md`, and
  `docs/pg-embedded-setup-unpriv-users-guide.md`.

Terminology used in this plan:

- Outbound transport: sending a reply or stream to the current connection.
- Outbound messaging: pushing a notification to a specific connection or a
  group of connections outside the request/response flow.

## Plan of Work

Stage A: Confirm current boundaries and APIs (no code changes).

Review current routing and command paths to identify where to inject outbound
context without touching domain logic. Inspect wireframe push/session types in
`docs/wireframe-users-guide.md` and in the wireframe crate API to confirm names
for `PushHandle`, `SessionRegistry`, and `ConnectionId`. Record any mismatches
as a decision or risk update.

Stage B: Define outbound traits and test doubles (small, verifiable diffs).

Create a new module, likely `src/server/outbound.rs`, with:

- `OutboundTransport` for responses tied to the current connection.
- `OutboundMessaging` for push/broadcast delivery.
- `OutboundPriority` enum to map to wireframe high/low priority queues.
- `OutboundTarget` newtype (for connection or user identifiers) to avoid
  leaking wireframe identifiers into the domain.
- `OutboundError` enum using `thiserror` with variants for queue closure,
  missing session, and serialization failure.

Include a test-only in-memory implementation (inside `#[cfg(test)]`) to assert
outbound calls with `rstest` and to validate error propagation. Add Rustdoc
examples that compile in doctest mode and reference only domain types.

Stage C: Wireframe adapter integration (minimal change to satisfy tests).

Capture the wireframe `PushHandle` on connection setup and store it in a
server-boundary adapter type implementing the outbound traits. The adapter can
live in `src/wireframe` (for wireframe-specific types) but must expose only the
trait object to domain code. Thread an outbound context (struct holding
`&dyn OutboundTransport` and `&dyn OutboundMessaging`) through
`process_transaction_bytes` into `Command::process_with_outbound`, keeping
`Command::process` as the compatibility entry point.

If an existing handler can emit a meaningful outbound message (e.g., a
post-login notification), use it to prove the interface. If no natural message
exists yet, add a minimal internal-only hook (feature-guarded or test-only) to
exercise the outbound trait without changing user-visible behaviour. Ensure
that no domain module imports wireframe types by checking `rg "wireframe::"`.

Stage D: Tests, documentation, and roadmap update.

Add unit tests with `rstest` for the outbound traits and wireframe adapter
implementation (happy path and error path). Add BDD scenarios using
`rstest-bdd` v0.3.2 under `tests/features/` that demonstrate outbound delivery
behaviour and failure handling from a user-observable perspective. For any
behavioural tests that touch the database, use the
`pg_embedded_setup_unpriv::test_support::test_cluster` fixture. Update
`docs/design.md` with the chosen trait shapes and reasoning, update
`docs/users-guide.md` if any new behaviour or configuration is exposed, and
mark roadmap item 1.4.4 as done.

## Concrete Steps

All commands run from the repository root. Pipe long outputs to a log using
`tee` with `/tmp/$ACTION-$(get-project)-$(git branch --show).out`. If the
`get-project` helper is unavailable, substitute `$(basename "$PWD")`.

1) Discovery

    rg -n "wireframe::" src
    rg -n "PushHandle|SessionRegistry|ConnectionId" \\
        docs/wireframe-users-guide.md

2) Add outbound traits and tests

    $EDITOR src/server/outbound.rs
    $EDITOR src/commands/mod.rs
    $EDITOR src/commands/handlers.rs

3) Wireframe adapter plumbing

    $EDITOR src/wireframe/protocol.rs
    $EDITOR src/wireframe/routes/mod.rs
    $EDITOR src/server/wireframe.rs

4) Tests and documentation

    $EDITOR tests/features/outbound_messaging.feature
    $EDITOR tests/outbound_messaging_bdd.rs
    $EDITOR docs/design.md
    $EDITOR docs/users-guide.md
    $EDITOR docs/roadmap.md

5) Formatting and verification

    make fmt | tee /tmp/fmt-$(get-project)-$(git branch --show).out
    make markdownlint | tee \\
        /tmp/markdownlint-$(get-project)-$(git branch --show).out
    make nixie | tee /tmp/nixie-$(get-project)-$(git branch --show).out
    make check-fmt | tee /tmp/check-fmt-$(get-project)-$(git branch --show).out
    make lint | tee /tmp/lint-$(get-project)-$(git branch --show).out
    make test | tee /tmp/test-$(get-project)-$(git branch --show).out

## Validation and Acceptance

Acceptance is met when:

- Domain modules compile without `wireframe` imports (`rg -n "wireframe::"` only
  returns files under `src/wireframe` or `src/server/wireframe.rs`).
- The outbound traits are exercised by unit tests using `rstest` covering
  success, missing-session, and delivery-failure paths.
- BDD scenarios using `rstest-bdd` demonstrate outbound messaging behaviour in
  a user-observable way and pass under `make test`.
- `make check-fmt`, `make lint`, and `make test` all pass.
- Documentation updates in `docs/design.md`, `docs/users-guide.md`, and
  `docs/roadmap.md` are formatted and linted.

## Idempotence and Recovery

Edits are safe to re-run. If a test fails, fix the issue and re-run the
relevant command. If documentation linting fails, run `make fmt` and re-run
`make markdownlint` and `make nixie`.

## Artifacts and Notes

Record key command outputs (test runs, lint runs) by keeping the `tee` logs
listed above. Summarize any failures and resolutions in `Decision Log`.

## Interfaces and Dependencies

Define outbound traits in `src/server/outbound.rs`:

    pub enum OutboundPriority { High, Low }

    pub struct OutboundConnectionId(u64);

    pub enum OutboundTarget {
        Current,
        Connection(OutboundConnectionId),
    }

    pub trait OutboundTransport {
        fn send_reply(
            &mut self,
            reply: Transaction,
        ) -> Result<(), OutboundError>;
    }

    pub trait OutboundMessaging {
        async fn push(
            &self,
            target: OutboundTarget,
            message: Transaction,
            priority: OutboundPriority,
        ) -> Result<(), OutboundError>;

        async fn broadcast(
            &self,
            message: Transaction,
            priority: OutboundPriority,
        ) -> Result<(), OutboundError>;
    }

The wireframe adapter should implement these traits and translate
`OutboundPriority` to wireframe queue priorities using `PushHandle`, backed by
a per-connection `WireframeOutboundConnection` and shared registry. Keep any
wireframe-specific types confined to `src/wireframe`, exposing only the trait
objects to domain code.

## Revision note

Initial draft created on 2026-01-17 to cover roadmap item 1.4.4. 2026-01-17
02:21Z: Updated progress, decisions, and interface signatures to match the
implemented outbound traits and wireframe adapter wiring.
