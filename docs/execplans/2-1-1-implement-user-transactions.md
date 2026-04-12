# Task 2.1.1: Implement user presence transactions

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises and discoveries`, `Decision log`, and
`Outcomes and retrospective` must be kept up to date as work proceeds.

Status: DRAFT

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 2.1.1 requires Hotline user-presence behaviour to move from the
current single-session request/reply model to full session-aware presence
updates. The immediate goal is to implement the user-list and user-update
transaction family so logged-in clients can discover who is online, observe
user-info changes in real time, and remove departed users without manual
refresh.

Success is observable when:

- `Get User Name List` (300) returns the current online roster using the
  Hotline payload format expected by clients.
- `Notify Change User` (301) is pushed to the correct peers on session join and
  user-info updates.
- `Notify Delete User` (302) is pushed to the remaining peers on logout or
  disconnect.
- `Get Client Info Text` (303) and `Set Client User Info` (304) behave per
  `docs/protocol.md`, including privilege and authentication checks.
- Agreement sequencing is aligned with `docs/protocol.md`: a session only
  becomes publicly online once the protocol says it is finalized.
- New unit tests use `rstest`, behaviour tests use `rstest-bdd`, and local
  PostgreSQL validation runs through the `pg_embedded_setup_unpriv` flow.
- `docs/design.md`, `docs/users-guide.md`, and `docs/roadmap.md` are updated
  before the work is closed.

## Constraints

- Follow `docs/protocol.md` exactly. Do not infer alternative Hotline
  semantics from client folklore when the repository documentation is clear.
- Preserve the hexagonal boundary described in `docs/design.md`. Domain code
  must not depend directly on `wireframe` types.
- Reuse the existing outbound messaging boundary
  (`src/server/outbound.rs`) rather than introducing ad hoc push logic in
  handlers.
- Keep session-lifecycle state explicit. Presence must distinguish
  authenticated-but-not-yet-online from fully online sessions.
- Add unit coverage with `rstest` for helpers, codecs, registries, and handler
  logic.
- Add behavioural coverage with `rstest-bdd` for user-observable flows where
  wire behaviour matters.
- Prefer the existing binary-backed integration harness in `test-util` for
  unsolicited `301/302` notification coverage.
- Use the PostgreSQL local-test path described in
  `docs/pg-embed-setup-unpriv-users-guide.md`; do not rely on a manually
  provisioned external PostgreSQL instance.
- Keep documentation in en-GB-oxendict spelling and wrap prose at 80 columns.
- Keep files under 400 lines by extracting helpers or new modules as needed.
- Run quality gates with `set -o pipefail` and `tee` so truncated terminal
  output does not hide failures.

## Tolerances (exception triggers)

- Wire-format drift: if implementation evidence conflicts with the
  SynHX-derived field `300` layout (`uid:u16`, `icon:u16`, `colour:u16`,
  `name_len:u16`, `name:name_len bytes`), stop and reconcile the wire format
  before implementation. Do not silently ship a second shape.
- Scope: if the work expands beyond presence transactions and agreement gating
  into private messaging or chat-room behaviour, stop and split the work.
- Interface churn: if satisfying logout/disconnect notifications requires a
  breaking public API change outside the existing server boundary, stop and
  present options.
- Harness churn: if binary-backed behavioural coverage requires a third major
  test-world rewrite, stop and isolate the harness work into a preparatory task.
- Retry loop: if `make lint` or `make test` still fail after two targeted fix
  cycles, stop and capture the failing logs.

## Risks

- Risk: presence notifications race with session registration or teardown,
  causing duplicate or missing `301/302` pushes. Severity: high. Likelihood:
  medium. Mitigation: centralize add/update/remove operations in a single
  presence registry and make notifications derive from the registry transition,
  not from scattered handler branches.

- Risk: current login flow authenticates immediately, while the protocol says
  the session becomes publicly online after agreement handling. Severity: high.
  Likelihood: medium. Mitigation: model explicit lifecycle phases
  (`Unauthenticated`, `AuthenticatedPendingAgreement`, `Online`) and gate
  `300/301/302` emission on the online phase only.

- Risk: the single-client binary BDD harness cannot reliably observe
  unsolicited pushes. Severity: medium. Likelihood: high. Mitigation: extend
  `WireframeBddWorld` for deterministic multi-client orchestration and explicit
  read windows rather than relying on opportunistic reads.

- Risk: the presence payload model may drift from the protocol if field IDs and
  packed structures are spread across handlers. Severity: medium. Likelihood:
  medium. Mitigation: introduce dedicated presence payload types/helpers with
  byte-level unit tests.

## Progress

- [x] (2026-04-10 00:00Z) Drafted ExecPlan for roadmap item 2.1.1 after
      reviewing `docs/roadmap.md`, `docs/protocol.md`, `docs/design.md`,
      testing guidance, and existing execplans.
- [x] (2026-04-12 00:00Z) Derived the field `300` wire layout from the SynHX
      client source (`hotline.h`, `hx_commands.c`), resolving the previous
      protocol ambiguity for "User Name with Info".
- [ ] Lock the exact protocol scope, including how roadmap wording
      "300-307 / agree-disagree flows" maps to documented transactions.
- [ ] Introduce the missing transaction and field models for presence payloads.
- [ ] Add shared session/presence registry infrastructure.
- [ ] Implement transaction handlers and server-initiated notifications.
- [ ] Extend binary-backed test harnesses for multi-client push assertions.
- [ ] Add unit, behavioural, and PostgreSQL-backed validation coverage.
- [ ] Update `docs/design.md`, `docs/users-guide.md`, and `docs/roadmap.md`.
- [ ] Run the full quality gates and capture logs with `tee`.

## Surprises and discoveries

- `src/transaction_type.rs` currently models transaction `300`, but not
  `301-304`; `109`, `121`, and `354` exist as enum variants but are not wired
  into command parsing.
- `src/field_id.rs` does not yet model the key presence fields used by
  `docs/protocol.md`, including `102`, `103`, `104`, `112`, `113`, `215`, and
  the repeated `300` field.
- `src/commands/mod.rs` still contains a `TODO` noting that
  `Command::process_with_outbound()` does not yet use the outbound messaging
  adapter for server-initiated notifications.
- `src/handler.rs` tracks only `user_id`, `privileges`, and
  `connection_flags`; there is no shared presence registry and no session data
  for nickname, icon, auto-response text, or finalized-online state.
- The binary-backed BDD world in `test-util/src/wireframe_bdd_world.rs` is
  currently optimized for a single connected client and synchronous reply
  reads; unsolicited multi-client push assertions will need more machinery.
- The SynHX client source resolves the field `300` payload shape. It defines a
  packed `hl_userlist_hdr` containing `uid`, `icon`, `color`, `nlen`, then the
  name bytes, all prefixed by the ordinary Hotline field header for type
  `0x012c`.
- The same SynHX user-list response parser also accepts `HTLS_DATA_CHAT_SUBJECT`
  (field `115`) alongside repeated field `300` entries, so a compatible
  transaction `300` reply may carry the main chat subject too.
- `docs/protocol.md` clearly documents `300-304` and agreement flow `109/121`,
  but the roadmap shorthand says `300-307` and "agree/disagree flows". That
  wording must be reconciled explicitly rather than assumed.

## Decision log

- Decision: treat `docs/protocol.md` as the source of truth over the roadmap
  shorthand. Implement the behaviour explicitly documented for `300-304` and
  the existing agreement/login sequencing (`109`, `121`, `354`), and treat any
  additional `305-307` semantics as out of scope unless repository evidence is
  found. Rationale: the roadmap item references `docs/protocol.md` and requires
  exact protocol behaviour, while the currently checked-in protocol guide does
  not define `305-307` in the reviewed sections. Date/Author: 2026-04-10 /
  Codex.

- Decision: model presence as adapter-owned runtime state, not database state.
  Rationale: online presence is ephemeral, per-connection, and must react to
  disconnect timing; persisting it in Diesel tables would complicate teardown
  and move transport timing concerns into storage code. Date/Author: 2026-04-10
  / Codex.

- Decision: use targeted pushes via stored `OutboundConnectionId` values rather
  than broad `broadcast()` calls for `301/302`. Rationale: protocol semantics
  require excluding the joining user from some broadcasts and targeting only
  remaining online peers on delete; per-connection targeting avoids extra
  outbound interface changes. Date/Author: 2026-04-10 / Codex.

- Decision: define "online" as the point after agreement completion, or
  immediately after login only when the effective session policy indicates no
  agreement is required. Rationale: this matches the lifecycle text in
  `docs/protocol.md` and prevents clients from seeing half-initialized sessions
  in the user list. Date/Author: 2026-04-10 / Codex.

- Decision: encode and decode field `300` using the SynHX-observed
  `hl_userlist_hdr` layout: `uid:u16`, `icon:u16`, `colour:u16`,
  `name_len:u16`, followed by `name_len` bytes of nickname, with the enclosing
  parameter length set to `8 + name_len`. Rationale: SynHX defines this exact
  structure in `hotline.h` and parses it in `hx_commands.c` for both self-info
  and user-list responses, giving concrete client-side wire evidence. Date/
  Author: 2026-04-12 / Codex.

## Outcomes and retrospective

Intended outcomes once implemented:

- Presence-aware clients receive protocol-correct join, update, and leave
  events without polling.
- Session lifecycle state becomes explicit and easier to extend for roadmap
  items `2.1.2` and `2.1.3`.
- The binary-backed test harness can validate unsolicited server pushes across
  multiple concurrent clients.

Retrospective placeholder:

- Implemented:
- Validation:
- Documentation:
- Follow-up work:

## Context and orientation

Primary files and modules in current state:

- `docs/roadmap.md`: roadmap source for task `2.1.1`.
- `docs/protocol.md`: protocol source for transactions `300-304` and agreement
  lifecycle `109/121/354`.
- `docs/design.md`: architecture and prior presence notes that already mark
  `300` as stubbed and `301/302` as planned.
- `src/transaction_type.rs`: current transaction enum; needs the rest of the
  presence family.
- `src/field_id.rs`: current field enum; missing several presence-related
  fields.
- `src/handler.rs`: shared session model; currently lacks presence metadata and
  finalized-online state.
- `src/login.rs`: current login implementation; authenticates immediately and
  does not yet participate in agreement gating or presence broadcasts.
- `src/commands/mod.rs`: current routing/dispatch entrypoint; outbound
  messaging remains unused here.
- `src/server/outbound.rs` and `src/wireframe/outbound.rs`: existing push
  abstraction and wireframe-backed outbound registry.
- `src/server/wireframe/mod.rs`: wireframe app factory; creates per-connection
  session and outbound state.
- `test-util/src/wireframe_bdd_world.rs`: binary-backed BDD harness that will
  need multi-client push support.
- `tests/wireframe_routing_bdd.rs` and `tests/payload_reject.rs`: current
  routing and payload validation coverage touching login and transaction `300`.

Current implementation gap summary:

- `GetUserNameList` exists only as a payload-reject path when clients send an
  unexpected payload; there is no successful presence implementation.
- No shared runtime structure currently knows which authenticated sessions are
  online, what names/icons they expose, or where to send unsolicited pushes.
- Disconnect cleanup is not tied to a presence registry, so `302` cannot be
  emitted today.
- The protocol-level agreement lifecycle is documented but not yet enforced by
  command handlers.

## Plan of work

### Stage A: lock protocol scope and payload model

Audit the exact semantics needed from the checked-in protocol guide and record
the chosen mapping in `docs/design.md` before large code changes begin.

This stage must answer:

- whether `2.1.1` means `300-304` plus agreement lifecycle handling, or whether
  repository evidence for `305-307` exists elsewhere;
- how to codify the now-derived field `300` ("User Name with Info") layout in
  MXD types and helpers;
- which fields are required for `301`, `302`, `303`, and `304`;
- when a session becomes visible to other clients.

Implementation work in this stage:

- Add or confirm the missing `FieldId` and `TransactionType` constants needed
  by the presence family.
- Introduce focused types for presence snapshots and user-info updates, rather
  than building those payloads from raw tuples throughout the codebase.

Validation gate for Stage A:

- A short design note is added to `docs/design.md` covering the presence
  payload model, online-state transition, and roadmap-scope interpretation.

### Stage B: add shared session lifecycle and presence registry support

Introduce a shared runtime registry that tracks every connection relevant to
presence notifications.

Registry responsibilities:

- store the outbound connection identifier used to push `301/302`;
- track lifecycle phase (`pending` vs `online`);
- track display metadata needed for `300/301` (user ID, name, icon, flags);
- support enumeration for `300`, targeted fan-out for `301/302`, updates for
  `304`, and removal on logout/disconnect.

Likely code changes:

- extend `Session` or add a colocated runtime struct for presence-specific
  metadata;
- create a shared registry in the wireframe app factory and make it available
  through routing/middleware context;
- couple registry cleanup to connection teardown so unexpected socket closure
  removes the user exactly once.

Validation gate for Stage B:

- `rstest` unit coverage proves add/update/remove/finalize semantics,
  duplicate-removal safety, and deterministic peer selection for fan-out.

### Stage C: implement the transaction handlers and agreement gating

Implement or correct the protocol handlers so request/reply semantics and
online-state transitions match `docs/protocol.md`.

Handler scope:

- `300 Get User Name List`: authenticated/online client receives the current
  roster encoded as repeated field `300` entries.
- `303 Get Client Info Text`: authenticated client requests another user's
  info; privilege checks and unhappy paths are explicit.
- `304 Set Client User Info`: current user updates nickname, icon, options, and
  optional auto-response text; runtime state is updated but account storage is
  not.
- Agreement sequencing: implement the missing `109`/`121`/`354` flow or revise
  the current login path so presence visibility is only triggered when the
  session is protocol-complete.

Important behavioural rules:

- failed login must not create an online session or emit `301`;
- a pending agreement session must not appear in `300` or receive presence
  broadcasts as a fully online peer;
- `304` changes must produce the same effective snapshot that `300` later
  returns.

Validation gate for Stage C:

- Unit tests cover happy, unhappy, and edge paths for `300`, `303`, `304`, and
  agreement finalization.

### Stage D: implement server-initiated `301/302` notification delivery

Use the existing outbound messaging abstraction to deliver unsolicited
transactions to the right clients at the right time.

Notification sources:

- session becomes online;
- session-visible metadata changes through `304`;
- session logs out or disconnects.

Notification rules:

- on join, send `301` to the other online clients, not back to the joining
  client that already obtains the roster via `300`;
- on metadata update, send `301` to all other relevant online clients;
- on logout/disconnect, send `302` to the remaining online clients after the
  departing session has been removed from the registry view;
- preserve transaction framing, routing IDs, and error-free push behaviour
  through the existing transport adapter.

Validation gate for Stage D:

- `rstest` unit tests confirm notification fan-out excludes the correct
  connection and produces protocol-correct payload fields.

### Stage E: extend behavioural and PostgreSQL-backed verification

Add the test coverage needed by the roadmap item and the repository testing
guides.

Unit-test coverage (`rstest`):

- presence payload encoding helpers;
- transaction parsing/building for new field IDs and variants;
- registry lifecycle operations;
- logout/disconnect cleanup and duplicate-removal edge cases;
- agreement-gating decisions and failed-login non-visibility.

Behavioural coverage (`rstest-bdd`), ideally against the running
`mxd-wireframe-server` binary:

- existing online client receives `301` when another user completes login;
- new client receives `300` with the already-online roster;
- user-info changes via `304` produce visible updates through `301`;
- disconnect or logout emits `302` to the remaining client;
- unhappy paths: failed login, pending agreement, unauthenticated requests, and
  unknown target user for `303`.

PostgreSQL-backed validation:

- run the relevant behavioural and integration suites with the embedded
  PostgreSQL backend enabled via the `pg_embedded_setup_unpriv` workflow
  described in `docs/pg-embed-setup-unpriv-users-guide.md`;
- keep the SQLite and wireframe-only feature sets green as part of `make test`.

Validation gate for Stage E:

- multi-client behavioural tests pass consistently for both the default backend
  path and the embedded PostgreSQL path.

### Stage F: documentation, roadmap closure, and quality gates

Update the docs and close the roadmap item only after implementation and
validation are complete.

Documentation changes:

- `docs/design.md`: record the final scope decision, presence registry design,
  and session online-state transition.
- `docs/users-guide.md`: describe any user-visible change in when a client
  appears online, how nickname/icon changes propagate, and what clients should
  expect on disconnect.
- `docs/roadmap.md`: mark `2.1.1` done with a completion note once the feature
  lands.

Required verification commands for implementation work:

```sh
set -o pipefail
make check-fmt 2>&1 | tee /tmp/2-1-1-check-fmt.log
make lint 2>&1 | tee /tmp/2-1-1-lint.log
make test 2>&1 | tee /tmp/2-1-1-test.log
make markdownlint 2>&1 | tee /tmp/2-1-1-markdownlint.log
```

Run `make fmt` before those commands when formatting fixes are needed, and run
`make nixie` if any Mermaid diagrams are added or changed.
