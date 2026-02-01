# Detect XOR-encoded text fields for SynHX clients

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: DRAFT

PLANS.md does not exist in this repository.

## Purpose / big picture

Enable compatibility with clients (including SynHX) that XOR-encode text
fields by transparently decoding inbound text parameters and encoding outbound
responses when required. Success is visible when SynHX parity tests cover
passwords, messages, and news bodies with the XOR toggle enabled, and when
`rstest` unit tests plus `rstest-bdd` v0.4.0 behavioural scenarios pass.
Documentation reflects the compatibility policy, and roadmap entry 1.5.1 is
marked done.

## Constraints

- Domain modules must remain free of wireframe imports and compatibility
  conditionals. XOR handling stays in the wireframe adapter boundary.
- No new external dependencies. If a new crate is required, stop and
  escalate.
- Every new module must start with a module-level `//!` comment.
- All new public APIs require Rustdoc comments with examples.
- Keep files under 400 lines; split modules if needed.
- Documentation uses en-GB-oxendict spelling and wraps prose at 80 columns.
- Use `rstest` for unit tests and `rstest-bdd` v0.4.0 for behavioural tests.
- Use `pg_embedded_setup_unpriv` as documented in
  `docs/pg-embed-setup-unpriv-users-guide.md` for Postgres-backed tests.
- Run `make check-fmt`, `make lint`, and `make test` before committing. If
  documentation changes, also run `make fmt`, `make markdownlint`, and
  `make nixie`.

## Tolerances (exception triggers)

- Scope: if more than 12 files change or net changes exceed 450 LOC, stop and
  escalate.
- Interface: if a public API outside `src/wireframe` or `src/transaction`
  must change, stop and escalate.
- Dependencies: if a new external dependency is required, stop and escalate.
- Iterations: if tests or lint still fail after two fix attempts, stop and
  escalate.
- Ambiguity: if multiple valid XOR detection rules or text-field lists exist,
  stop and present options with trade-offs.

## Risks

- Risk: XOR detection rules (sub-version mapping, sub-protocol tag, or config
  toggle) are ambiguous. Severity: medium. Likelihood: medium. Mitigation:
  cross-check `docs/migration-plan-moving-mxd-protocol-implementation-to-
  wireframe.md`, `docs/protocol.md`, and hx behaviour; document the decision
  in `docs/design.md` and the Decision Log; escalate if ambiguity remains.
- Risk: “message” field mapping is unclear because message transactions are
  not yet implemented. Severity: medium. Likelihood: medium. Mitigation:
  confirm which field IDs represent message text (likely field 101) and record
  the mapping; add tests that exercise the chosen field and update
  `FieldId` or helper lists accordingly.
- Risk: outbound XOR encoding could corrupt binary payloads or mismatched
  parameter lists. Severity: high. Likelihood: low. Mitigation: only apply
  XOR to decoded parameter payloads and known text fields; skip transformation
  when payload parsing fails and log at debug level.
- Risk: double-encoding or missing decode path if transforms are applied in
  multiple layers. Severity: medium. Likelihood: medium. Mitigation: centralise
  inbound decode in transaction routing and outbound encode in a single hook
  (preferably `HotlineProtocol::before_send`) with clear tests.
- Risk: hx-based validator tests are flaky or skipped when hx is missing.
  Severity: low. Likelihood: medium. Mitigation: keep tests skippable with
  clear messages, and rely on unit/BDD tests for CI coverage.

## Progress

- [x] (2026-02-01 00:00Z) Drafted ExecPlan and gathered context.
- [ ] (2026-02-01 00:00Z) Confirm XOR detection policy and text-field list.
- [ ] (2026-02-01 00:00Z) Implement adapter-level XOR decode/encode pipeline.
- [ ] (2026-02-01 00:00Z) Add rstest unit coverage and rstest-bdd scenarios.
- [ ] (2026-02-01 00:00Z) Extend validator hx parity tests.
- [ ] (2026-02-01 00:00Z) Update documentation and roadmap entry.
- [ ] (2026-02-01 00:00Z) Run quality gates and record results.

## Surprises & discoveries

- (To be populated during implementation.)

## Decision log

- Decision: implement XOR transforms in the wireframe adapter (routing
  middleware for inbound decode, protocol hook for outbound encode) so the
  domain remains unaware of client quirks. Rationale: preserves hexagonal
  boundaries and keeps protocol shims at the edge. Date/Author: 2026-02-01 /
  Codex.
- Decision: derive the XOR compatibility mode from handshake metadata and keep
  the mapping in a dedicated compatibility policy module. Rationale: keeps the
  logic centralised and testable, and avoids scattering `if sub_version` checks
  across handlers. Date/Author: 2026-02-01 / Codex.
- Decision: treat hx validator tests as parity checks and back them with
  deterministic unit/BDD tests for CI reliability. Rationale: external client
  tests are valuable but inherently flaky. Date/Author: 2026-02-01 / Codex.

## Outcomes & retrospective

- (To be populated at completion.)

## Context and orientation

Relevant implementation areas:

- `src/wireframe/handshake.rs` stores handshake metadata in a thread-local
  `ConnectionContext` during the preamble success hook.
- `src/server/wireframe.rs` uses `take_current_context()` to build a per-
  connection app and currently discards handshake metadata.
- `src/wireframe/protocol.rs` owns `HotlineProtocol` and provides
  `before_send`, which is the intended outbound shim location.
- `src/wireframe/routes/mod.rs` parses inbound frames in
  `process_transaction_bytes` and is the right place to apply inbound XOR
  decoding before `Command::from_transaction`.
- `src/transaction/params.rs` provides parameter encode/decode helpers used by
  command parsing and will be reused for XOR-aware rewrites.
- `src/field_id.rs` enumerates known parameter IDs; the text-field list should
  live alongside or reference these IDs.
- `src/wireframe/test_helpers/mod.rs` offers helpers for building frames and
  will simplify XOR tests.
- `validator/tests/login.rs` shows the hx harness; new parity tests should
  follow that style.

Documentation references:

- `docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md`
  (SynHX compatibility and XOR notes).
- `docs/design.md` (compatibility expectations and testing strategy).
- `docs/protocol.md` (field ID semantics and parameter formats).
- `docs/users-guide.md` (user-visible server behaviour).
- `docs/rstest-bdd-users-guide.md` (v0.4.0 usage patterns).
- `docs/pg-embed-setup-unpriv-users-guide.md` (Postgres test setup).

## Plan of work

### Stage A: Confirm compatibility policy and field mapping

Review the migration plan and protocol documentation to determine the exact
XOR trigger (sub-version mapping, sub-protocol tag, or configuration flag).
Inventory which field IDs represent text and must be XOR-transformed, paying
special attention to password (field 106), message body (likely field 101),
and news article bodies (field 333). If any mapping is ambiguous, pause and
escalate for clarification. Record the final policy in `docs/design.md` and
this plan’s Decision Log.

### Stage B: Implement XOR decode/encode at the adapter boundary

Introduce a compatibility module (for example `src/wireframe/compat.rs` or
`src/wireframe/xor.rs`) that defines the XOR mode, the text-field list, and
helpers that transform parameter payloads by XOR-ing only the relevant fields.
Prefer pure functions that accept decoded parameter vectors and return new
payload bytes so they are easy to unit test.

Plumb handshake metadata into the wireframe adapter so the compatibility mode
is available in both inbound and outbound paths. Options include:

- storing the derived compatibility mode inside the per-connection
  `HotlineProtocol` instance so `before_send` can encode responses, and
- adding a compatibility field to `TransactionMiddleware`/`RouteContext` so
  `process_transaction_bytes` can decode inbound frames before parsing
  commands.

Ensure outbound encoding happens exactly once. The simplest approach is to
perform outbound XOR in `HotlineProtocol::before_send`, which is called for
responses and push traffic. For inbound frames, decode in
`process_transaction_bytes` before `Command::from_transaction` so the domain
sees plaintext. Skip transformation when payload parsing fails and return the
existing error replies.

### Stage C: Unit tests with rstest

Add rstest unit coverage for the XOR helper module:

- XOR round-trips restore original bytes.
- Only configured text field IDs are transformed.
- Multiple values for the same field (e.g. lists) are all transformed.
- Empty payloads or missing fields are no-ops.
- Invalid UTF-8 after XOR is reported as `TransactionError::InvalidParamValue`.

Add unit tests around the routing pipeline to assert that XOR decoding makes
login/password parsing succeed when the client sends XOR-encoded values. Add
unit coverage for outbound encoding (for example by invoking `before_send`
with a frame that contains text parameters and verifying the payload bytes are
XOR-ed).

### Stage D: Behavioural tests with rstest-bdd v0.4.0

Create new `.feature` files under `tests/features/` describing XOR behaviour
for login and news payloads. Implement step definitions using
`rstest_bdd_macros` that:

- start a wireframe server with a known XOR-triggering handshake,
- send a login frame with XOR-encoded password and assert successful reply,
- post or fetch a news article with XOR-encoded body and assert the
  round-tripped body matches plaintext,
- cover an unhappy path where XOR is disabled and the same payload is rejected
  or results in invalid UTF-8 errors.

Reuse `wireframe::test_helpers::build_frame` and direct TCP connections for
precise byte-level assertions. Keep scenarios small and focused.

### Stage E: SynHX parity tests (validator crate)

Extend the `validator` crate with hx-driven tests that verify XOR behaviour for
passwords, messages, and news bodies. Determine the correct hx commands for
sending a message and posting/fetching news, and adjust the test setup to seed
necessary data. Tests should remain skippable when hx is not installed, but
should assert behaviour when it is present. If a message command is not
available, stop and escalate with the observed limitation.

### Stage F: Documentation and roadmap updates

Update `docs/design.md` with the XOR compatibility policy (trigger, field
coverage, and transform location). Update `docs/users-guide.md` if the server
behaviour or configuration surface changes (for example, if a new toggle or
logging is added). Mark roadmap task 1.5.1 as done in `docs/roadmap.md` with
an explicit date and summary.

### Stage G: Verification and quality gates

Run formatting, lint, and test suites using Makefile targets with `tee`
logging and `set -o pipefail`. Run validator tests separately. Use
`pg_embedded_setup_unpriv` before Postgres tests as documented.

## Concrete steps

All commands run from the repository root unless noted. Use `set -o pipefail`
with `tee` to preserve exit codes.

1) Discovery and design

    rg -n "sub_version|HandshakeMetadata|take_current_context" src/wireframe
    rg -n "before_send" src/wireframe/protocol.rs
    rg -n "process_transaction_bytes|parse_transaction" \\
        src/wireframe/routes/mod.rs
    rg -n "FieldId" src/field_id.rs
    rg -n "encode_params|decode_params" src/transaction/params.rs
    rg -n "hx|validator" validator/tests

2) Implement XOR compatibility module and plumbing

    $EDITOR src/wireframe/compat.rs
    $EDITOR src/server/wireframe.rs
    $EDITOR src/wireframe/protocol.rs
    $EDITOR src/wireframe/routes/mod.rs

3) Unit tests

    $EDITOR src/wireframe/routes/tests/xor_compat.rs
    $EDITOR src/wireframe/compat.rs

4) Behaviour tests (rstest-bdd)

    $EDITOR tests/features/xor_text_fields.feature
    $EDITOR tests/wireframe_xor_compat.rs

5) Validator parity tests

    $EDITOR validator/tests/xor_compat.rs

6) Documentation and roadmap

    $EDITOR docs/design.md
    $EDITOR docs/users-guide.md
    $EDITOR docs/roadmap.md

7) Postgres setup (once per machine)

    set -o pipefail
    cargo install --locked pg-embed-setup-unpriv | tee /tmp/pg-embed-install.log
    pg_embedded_setup_unpriv 2>&1 | tee /tmp/pg-embed-setup.log

8) Quality gates

    set -o pipefail
    make fmt | tee /tmp/fmt-$(basename "$PWD").log
    make markdownlint | tee /tmp/markdownlint-$(basename "$PWD").log
    make nixie | tee /tmp/nixie-$(basename "$PWD").log
    make check-fmt | tee /tmp/check-fmt-$(basename "$PWD").log
    make lint | tee /tmp/lint-$(basename "$PWD").log
    make test | tee /tmp/test-$(basename "$PWD").log

9) Validator tests (hx required)

    set -o pipefail
    cargo test -p validator | tee /tmp/validator-$(basename "$PWD").log

## Validation and acceptance

Quality criteria (what “done” means):

- XOR compatibility is enabled for the intended client versions and only the
  selected text fields are transformed.
- Password, message, and news body paths succeed with XOR mode enabled, and
  the same inputs fail or are rejected when XOR mode is disabled.
- New rstest unit tests and rstest-bdd scenarios pass.
- `make check-fmt`, `make lint`, and `make test` pass.
- Documentation updates (if any) pass `make fmt`, `make markdownlint`, and
  `make nixie`.
- Validator hx parity tests pass (or skip with a clear message when hx is not
  installed).
- `docs/roadmap.md` marks task 1.5.1 as done.

## Idempotence and recovery

Steps are safe to re-run. If a test fails, fix the issue and re-run the
relevant command. `pg_embedded_setup_unpriv` can be run multiple times and is
safe; reuse its output across test runs.

## Artifacts and notes

Keep the `tee` logs listed in the concrete steps. If any command fails, record
what happened and the resolution in the Decision Log.

## Interfaces and dependencies

No new external dependencies are expected. The compatibility module should
expose a small surface such as:

    pub struct XorCompatibility {
        enabled: bool,
    }

    impl XorCompatibility {
        pub fn from_handshake(handshake: &HandshakeMetadata) -> Self { ... }
        pub fn decode_payload(&self, payload: &[u8]) ->
            Result<Vec<u8>, TransactionError> { ... }
        pub fn encode_payload(&self, payload: &[u8]) ->
            Result<Vec<u8>, TransactionError> { ... }
    }

Use `TransactionError` for parse failures so routing can reuse existing error
handling. Keep the compatibility module within `src/wireframe` to avoid
introducing wireframe dependencies into the domain core.
