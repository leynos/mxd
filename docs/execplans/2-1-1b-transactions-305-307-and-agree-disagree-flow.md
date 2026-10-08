# Implement agreement-gated session lifecycle parity

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: DRAFT

Implementation must not begin until this plan is explicitly approved.

## Purpose / Big Picture

Roadmap item `2.1.1b` completes the remaining session lifecycle work after
`2.1.1a`, which shipped the documented user-presence transactions `300-304`.
After this change, the Wireframe server should distinguish three user-visible
states:

- an unauthenticated connection, which cannot use presence operations;
- an authenticated but agreement-gated connection, which is known to the server
  but not yet visible in the user list; and
- an online connection, which has either bypassed the server agreement through
  the `NO_AGREEMENT` privilege or completed Agreement Acceptance transaction
  `121`.

Success is observable when a test user whose effective privileges do not
include `NO_AGREEMENT` logs in, receives the server agreement step, remains
absent from `Get User Name List` transaction `300`, does not trigger
`Notify Change User` transaction `301`, and becomes visible only after sending
`Agreed` transaction `121`. On completion, the server sends `User Access`
transaction `354` and then the usual presence lifecycle behaves as documented
in `docs/protocol.md`. Declining or abandoning the agreement is observable as a
closed or cleaned-up pending session that never appears online and never emits
`301` or `302`.

## Constraints

The implementation must follow these hard constraints.

- Treat `docs/protocol.md` as the protocol source of truth for this repository.
  Do not invent semantics for transaction identifiers that are not documented.
- Keep the plan and implementation aligned with `docs/roadmap.md` item
  `2.1.1b`, which depends on completed item `2.1.1a`.
- Preserve the hexagonal boundary from `docs/design.md`: session policy,
  privilege calculation, and presence state transitions belong in core modules;
  Wireframe-specific frame handling remains an adapter concern.
- Do not move database, Diesel, Wireframe, or TCP types into domain-only session
  policy code.
- Reuse the existing outbound messaging boundary in `src/server/outbound.rs`
  and the presence registry in `src/presence.rs` for server-initiated `301`,
  `302`, and `354` traffic.
- Use `rstest` for unit tests and `rstest-bdd` for behavioural tests where the
  behaviour is externally observable.
- Use `pg-embed-setup-unpriv` through the documented
  `docs/pg-embed-setup-unpriv-users-guide.md` flow for local PostgreSQL
  validation. The prompt names `docs/pg-embedded-setup-unpriv-users-guide.md`,
  but the repository file is `docs/pg-embed-setup-unpriv-users-guide.md`.
- Use a bounded model checker when adding or changing state-transition
  invariants. For this task, the existing Stateright session model in
  `crates/mxd-verification/src/session_model` is the right target; Kani is
  reserved for small pure codec or parser helpers.
- Use the published `diesel-cte-ext` crate only if this work introduces
  hierarchical relational logic. The current agreement and privilege lookup
  path is expected to be a flat user/permission query, so adding hierarchy
  would exceed this plan.
- Update `docs/users-guide.md` for user-visible server behaviour changes and
  `docs/developers-guide.md` for internal lifecycle, testing, or API changes.
- Mark roadmap item `2.1.1b` done in `docs/roadmap.md` only after the feature,
  tests, documentation, and quality gates all pass.
- Keep documentation in en-GB-oxendict spelling, with Markdown paragraphs
  wrapped at 80 columns.
- Run formatting, linting, tests, and documentation gates sequentially through
  Makefile targets with `tee` logs in `/tmp`.

## Tolerances

These thresholds trigger escalation. When one is reached, stop implementation,
record the issue in `Decision Log`, and wait for direction.

- Protocol ambiguity: if no repository source defines transactions `305-307`
  beyond the roadmap shorthand, do not implement fabricated `305-307`
  semantics. Present the evidence and choose between documenting them as
  intentionally undefined, correcting the roadmap item, or adding an
  authoritative protocol update before coding them.
- Disagree ambiguity: if a real client sends an explicit disagreement
  transaction not represented by `121`, disconnect, or connection close, stop
  and identify the transaction before handling it.
- Scope: if the change expands into idle timers, away state, private messaging,
  chat-room membership, or admin session termination, split that work into the
  later roadmap items instead of continuing.
- Interface churn: if a public crate API, CLI option, or database migration
  becomes necessary, keep it additive. If a breaking change is required, stop.
- Persistence: if agreement gating requires an account schema redesign rather
  than reading existing `permissions` / `user_permissions` rows or using an
  additive option, stop.
- Verification: if extending the Stateright model requires modelling idle or
  away behaviour from roadmap item `2.1.2`, stop and defer that model breadth
  to roadmap item `2.1.4`.
- Test iterations: if `make lint` or `make test` still fail after two targeted
  fix cycles, stop and capture the failing logs.
- File size: if any touched Rust source file would exceed 400 lines, extract a
  cohesive helper module instead of suppressing the rule.
- Dependencies: if a new external crate is needed, stop unless it is already
  mandated by the prompt or present in the workspace.

## Risks

- Risk: `docs/protocol.md` documents `109`, `121`, `300-304`, and `354`, but not
  `305-307`. Severity: high. Likelihood: high. Mitigation: make protocol
  reconciliation the first milestone and do not implement undocumented
  transaction names without an authoritative source.

- Risk: current login always adds `Privileges::NO_AGREEMENT`, making the
  `PendingAgreement` phase unreachable for real accounts. Severity: high.
  Likelihood: high. Mitigation: introduce a narrow effective-privilege lookup
  or policy seam so tests and configured accounts can exercise agreement-gated
  login while preserving default compatibility where appropriate.

- Risk: transaction `121` is present in `TransactionType`, but command parsing
  currently treats it as unknown. Severity: high. Likelihood: high. Mitigation:
  add an `Agreed` command path that reuses the existing `SetClientUserInfo`
  parsing shape, finalizes the pending session, emits `354`, and upserts
  presence exactly once.

- Risk: `User Access` transaction `354` must be server-initiated, but the
  current request/reply path primarily finalizes a single reply buffer.
  Severity: medium. Likelihood: medium. Mitigation: send `354` through the
  existing outbound transport or reply buffer immediately after `121`, and
  cover ordering with router-level tests.

- Risk: agreement acceptance and presence insertion can double-insert a session
  if login and `121` both share online-finalization logic. Severity: medium.
  Likelihood: medium. Mitigation: centralize finalization in a single helper
  that returns whether a session newly became online.

- Risk: the behavioural harness may not reliably observe unsolicited pushes
  across real sockets. Severity: medium. Likelihood: medium. Mitigation: use
  the existing router-level `PresenceWorld` for deterministic `rstest-bdd`
  coverage and add end-to-end socket coverage only where the change affects
  externally observable network ordering.

## Progress

- [x] 2026-05-01T00:27:28Z: Created the initial draft plan after reading
  `AGENTS.md`, `docs/roadmap.md`, `docs/protocol.md`, the completed `2.1.1a`
  ExecPlan, current session/presence code, and testing guidance.
- [x] 2026-05-01T00:27:28Z: Used a Wyvern agent team for read-only planning
  reconnaissance. The roadmap/protocol agent and code-structure agent
  completed; the broad documentation agent exceeded context and was replaced by
  local, narrower document reads.
- [ ] Obtain explicit approval for this plan.
- [ ] Reconcile the undocumented `305-307` roadmap wording before code changes.
- [ ] Add tests that fail on the current missing agreement acceptance flow.
- [ ] Implement agreement acceptance and online finalization.
- [ ] Add or adjust bounded verification for the session lifecycle invariant.
- [ ] Update user and developer documentation.
- [ ] Run all quality gates with captured logs.
- [ ] Mark roadmap item `2.1.1b` done after the feature passes all gates.

## Surprises & Discoveries

- `docs/roadmap.md` lines 329-333 require transactions `305-307` and the
  agree/disagree flow, but `docs/protocol.md` does not contain transaction
  sections for `305`, `306`, or `307`.
- `docs/protocol.md` lines 248-310 define the agreement lifecycle as `109`
  `Show Agreement`, `121` `Agreed`, and follow-up `354` `User Access`.
- `docs/protocol.md` lines 393-548 define presence lifecycle transactions
  `300-304`; these were implemented in the preceding `2.1.1a` work.
- `src/transaction_type.rs` already maps `109`, `121`, `300-304`, and `354`,
  but it has no dedicated variants for `305-307`.
- `src/wireframe/route_ids.rs` includes route `121`, so the adapter can receive
  Agreement Acceptance frames, but `src/commands/parsing.rs` currently falls
  through to `Command::Unknown` for `TransactionType::Agreed`.
- `src/login.rs` currently applies
  `Privileges::default_user() | Privileges::NO_AGREEMENT`, so normal login
  bypasses agreement gating even though `Session::apply_login` can enter
  `PendingAgreement`.
- `src/presence.rs` already models `SessionPhase::{Unauthenticated,
  PendingAgreement, Online}` and only returns a `PresenceSnapshot` for online,
  list-visible sessions.
- `docs/developers-guide.md` already explains the intended `PendingAgreement`
  and `Online` semantics, but the runtime still lacks a real `121` finalization
  path.

## Decision Log

- Decision: draft this as a plan only and leave status as `DRAFT`.
  Rationale: the user explicitly stated that the plan must be approved before
  it is implemented. Date/Author: 2026-05-01 / Codex.

- Decision: make protocol reconciliation the first implementation milestone.
  Rationale: implementing transaction identifiers `305-307` without repository
  semantics would violate the protocol-source constraint and create behaviour
  future clients cannot reason about. Date/Author: 2026-05-01 / Codex.

- Decision: treat "disagree" as connection close or abandonment unless an
  explicit protocol transaction is identified. Rationale: `docs/protocol.md`
  defines the accept path through `121` but does not define a disagreement
  transaction; a client that does not accept must not become visible.
  Date/Author: 2026-05-01 / Codex.

- Decision: use Stateright, not Kani, for the agreement-gating state invariant.
  Rationale: the invariant is about ordering across session states and peers;
  `docs/verification-strategy.md` routes concurrency or ordering risks to
  Stateright and reserves Kani for local pure helper invariants. Date/Author:
  2026-05-01 / Codex.

## Skill And Documentation Signposts

Use these skills while implementing this plan:

- `leta`: navigate Rust symbols and references before editing.
- `rust-router`: route any Rust-specific design question to the smallest useful
  Rust skill.
- `rust-errors`: use if command or repository errors need new semantic error
  enums.
- `rust-async-and-concurrency`: use if agreement finalization touches async
  ordering, locks, retries, or outbound delivery.
- `hexagonal-architecture`: protect the boundary between session policy,
  persistence, and Wireframe adapters.
- `nextest`: use if test selection or nextest failure triage is needed.
- `commit-message`: use for the required file-based commit workflow.

Use these repository documents as source material:

- `docs/roadmap.md` for the roadmap item and completion checkbox.
- `docs/protocol.md` for transactions `109`, `121`, `300-304`, and `354`.
- `docs/design.md` for architecture, current login policy notes, and presence
  lifecycle design.
- `docs/rust-testing-with-rstest-fixtures.md` for unit-test fixture style.
- `docs/rust-doctest-dry-guide.md` for doctest conventions if public helpers
  need examples.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` for test seams.
- `docs/verification-strategy.md` for choosing Stateright versus Kani.
- `docs/pg-embed-setup-unpriv-users-guide.md` for PostgreSQL local setup.
- `docs/wireframe-users-guide.md` for Wireframe server testing context.
- `docs/ortho-config-users-guide.md` if configuration is added.
- `docs/rstest-bdd-users-guide.md` for scenario bindings.
- `docs/users-guide.md` and `docs/developers-guide.md` for documentation
  updates.

## Context And Orientation

Current code already contains most of the presence foundation from `2.1.1a`.
`src/presence.rs` owns the in-memory online registry and payload builders for
`300`, `301`, `302`, and `303`. `src/handler.rs` owns per-connection `Session`
state, including the `SessionPhase` lifecycle. `src/commands/handlers.rs`
contains the login and presence command handlers that update the registry and
push notifications. `src/wireframe/router.rs` is the public Wireframe routing
entrypoint and passes a `CommandContext` into command execution.

The main missing path is agreement finalization. `src/login.rs` authenticates a
user, seeds default privileges plus `NO_AGREEMENT`, and therefore immediately
makes ordinary sessions online. `src/transaction_type.rs` recognises `Agreed`
transaction `121`, and `src/wireframe/route_ids.rs` registers route `121`, but
`src/commands/parsing.rs` has no `Agreed` command branch. The implementation
must make a session without `NO_AGREEMENT` stay in `PendingAgreement`, parse
`121`, apply the same nickname/icon/options fields used by `304`, send `354`,
and then publish the presence snapshot.

The `users` table currently stores only `id`, `username`, and `password`.
However, the schema also contains `permissions` and `user_permissions`, and
`docs/design.md` notes that the login flow still uses default privileges until
a later auth-focused task. For this feature, the smallest useful seam is an
effective-privilege policy or repository helper that can preserve current
default behaviour while allowing tests and configured accounts to omit
`NO_AGREEMENT`.

## Plan Of Work

### Milestone 1: Reconcile protocol scope

Before editing code, re-run a focused audit:

```sh
rg -n "\b(305|306|307|Agreement|Agreed|UserAccess|No Agreement)\b" docs src tests
```

Confirm whether any in-repository document, test, or source file defines
transactions `305-307`. If none does, update this plan's `Decision Log` with
the evidence and proceed only with documented agreement lifecycle work (`109`,
`121`, and `354`) plus the already documented presence lifecycle (`300-304`).
If authoritative semantics for `305-307` are found, add their names, fields,
and expected behaviours to this plan before implementation.

The milestone is complete when the implementing agent can state the exact
transaction scope without relying on guesswork.

### Milestone 2: Establish failing tests

Add red tests before production changes.

Unit tests with `rstest` should cover:

- `Session::apply_login` leaves a user without `NO_AGREEMENT` in
  `PendingAgreement`;
- pending sessions do not produce `PresenceSnapshot`;
- applying agreement fields from transaction `121` sets display name, icon,
  options, and auto-response consistently with `304`;
- agreement finalization transitions exactly once from `PendingAgreement` to
  `Online`;
- `121` before login, duplicate `121`, and malformed `121` payloads produce
  explicit errors or no-op behaviour without publishing presence.

Behavioural tests with `rstest-bdd` should cover:

- a `NO_AGREEMENT` account keeps the current immediate-online behaviour;
- an agreement-gated account logs in successfully but is absent from the roster
  until `121`;
- a peer receives `301` only after the gated account sends `121`;
- a pending account that disconnects produces no `301` or `302`;
- after `121`, the client receives `354` and can request `300` successfully.

End-to-end coverage should be added if the implementation changes the real
socket workflow or observable transaction ordering beyond router-level
behaviour. Use the existing binary-backed Wireframe harness where stable, but
prefer deterministic router-level BDD for unsolicited push assertions.

### Milestone 3: Add a narrow agreement policy boundary

Introduce the smallest policy seam needed to decide whether an authenticated
account requires agreement. Prefer one of these paths, in order:

1. Read effective privileges from existing `permissions` and `user_permissions`
   rows when present, falling back to the current default privileges plus
   `NO_AGREEMENT` when no account-specific rows exist.
2. If the existing permission tables are not ready for login use, add an
   internal test-only or configuration-backed policy seam that allows
   agreement-gated accounts to be exercised without changing the public CLI.

Keep the policy in core code and hide Diesel-specific lookup inside `src/db`.
The login handler should consume a domain value such as `Privileges`, not raw
database rows.

This milestone is complete when an implementation can create both immediate and
agreement-gated sessions without hard-coding `NO_AGREEMENT` into every login.

### Milestone 4: Implement `121` agreement finalization

Add a first-class `Command::Agreed` path. Reuse the parser shape for
`SetClientUserInfo` because `docs/protocol.md` gives `121` the same visible
fields: name `102`, icon `104`, options `113`, and optional auto-response `215`.

The handler should:

- require an authenticated `PendingAgreement` session;
- reject or no-op an unauthenticated session;
- reject or no-op an already-online session without duplicating presence;
- apply nickname, icon, options, and auto-response;
- transition the session to `Online`;
- build and send a `354 User Access` transaction containing field `110`;
- upsert the resulting `PresenceSnapshot`;
- notify existing online peers with `301`;
- leave the client able to request `300`.

If `docs/protocol.md` requires no direct reply for `121`, preserve that
behaviour. If the current Wireframe middleware requires a reply buffer to avoid
client hangs, document the repository-specific decision in `Decision Log` and
cover the exact observable behaviour in tests.

### Milestone 5: Implement agreement presentation and disagreement cleanup

Complete the server side of `109 Show Agreement` for agreement-gated sessions.
The server should send enough fields for the client to understand the agreement
state:

- field `101` for agreement text when text is configured;
- field `154` set to `1` when no agreement text is available, if that field is
  added to `FieldId`;
- banner fields only where already supported by the compatibility layer or
  explicitly required by `docs/protocol.md`.

If no configuration source for agreement text exists, prefer an additive
`AppConfig` field only if the behaviour cannot be represented by the current
server defaults. Any new option must be documented in `docs/users-guide.md` and
`docs/developers-guide.md`, and must include OrthoConfig tests.

For disagreement, implement the documented observable effect rather than an
undocumented packet: a pending session that closes or disconnects is removed
without becoming online and without producing `301` or `302`. If a real client
or repository source identifies an explicit disagreement transaction, return to
Milestone 1 before implementing it.

### Milestone 6: Extend bounded verification

Extend `crates/mxd-verification/src/session_model` so the model includes an
agreement-gated login and an `Agreed` action. The model should check at least
these invariants:

- pending sessions are never visible in the roster;
- privileged or presence effects do not occur before authentication;
- `Agreed` is the only transition from `PendingAgreement` to `Online`;
- disconnecting while pending produces no online-removal event;
- duplicate `Agreed` does not duplicate online presence.

Do not model idle timers or away flags here; that belongs to roadmap items
`2.1.2` and `2.1.4`.

### Milestone 7: Update documentation and roadmap

Update `docs/users-guide.md` to describe the operator-visible behaviour: which
accounts bypass agreement, which accounts must accept it, what clients see
before and after `121`, and what happens if the agreement is abandoned.

Update `docs/developers-guide.md` to describe the internal lifecycle and the
new test seams. If a configuration field or privilege lookup is added, document
how to create agreement-gated test accounts for SQLite and PostgreSQL.

Update `docs/design.md` only if the implementation changes the documented
architecture or revises the current statement that login always grants
`NO_AGREEMENT`.

Finally, mark `docs/roadmap.md` item `2.1.1b` done only after all validation
below succeeds.

## Validation

Run gates sequentially and capture output with `tee`; do not run tests, lint,
or format checks in parallel.

Use logs with names like:

```sh
/tmp/check-fmt-mxd-feat-plan-transactions-flow.out
/tmp/lint-mxd-feat-plan-transactions-flow.out
/tmp/test-mxd-feat-plan-transactions-flow.out
/tmp/markdownlint-mxd-feat-plan-transactions-flow.out
/tmp/nixie-mxd-feat-plan-transactions-flow.out
```

Required gates:

```sh
set -o pipefail
make check-fmt 2>&1 | tee /tmp/check-fmt-mxd-feat-plan-transactions-flow.out
make lint 2>&1 | tee /tmp/lint-mxd-feat-plan-transactions-flow.out
make test 2>&1 | tee /tmp/test-mxd-feat-plan-transactions-flow.out
make markdownlint 2>&1 | tee /tmp/markdownlint-mxd-feat-plan-transactions-flow.out
make nixie 2>&1 | tee /tmp/nixie-mxd-feat-plan-transactions-flow.out
```

Before `make test`, prepare PostgreSQL when PostgreSQL coverage is required:

```sh
export PG_VERSION_REQ="=16.4.0"
export PG_RUNTIME_DIR="/var/tmp/pg-embedded-setup-unpriv/install"
export PG_DATA_DIR="/var/tmp/pg-embedded-setup-unpriv/data"
export PG_SUPERUSER="postgres"
export PG_PASSWORD="postgres_pass"
export PG_TEST_BACKEND="postgresql_embedded"
pg_embedded_setup_unpriv
```

Run the verification suite through the Makefile target included in `make test`.
If a new Kani harness is added for a pure helper, also run the targeted
`cargo kani` command and record it here before completion.

## Expected Outcomes

When the implementation is complete:

- agreement-gated accounts are authenticated but not online after login;
- `121` finalizes a pending session exactly once;
- `354` is sent after agreement completion with the effective privilege bitmap;
- `300`, `301`, `302`, `303`, and `304` continue to match
  `docs/protocol.md`;
- abandoning the agreement leaves no published presence behind;
- unit, behavioural, PostgreSQL-backed, and verification tests cover happy,
  unhappy, and edge paths;
- `docs/users-guide.md`, `docs/developers-guide.md`, and `docs/roadmap.md`
  reflect the shipped behaviour.

## Outcomes & Retrospective

This section is intentionally empty while the plan is in `DRAFT`. During
implementation, record what changed, which validation commands passed, which
logs contain the evidence, and any follow-up roadmap items that remain out of
scope.
