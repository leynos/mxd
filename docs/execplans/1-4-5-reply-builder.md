# Provide reply builder for Hotline error propagation

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md does not exist in this repository.

## Purpose / Big Picture

Provide a reply builder in the wireframe routing layer so error replies mirror
Hotline semantics and logging conventions. Success is visible when error
replies preserve the originating transaction ID (where the header is present)
and error paths emit structured `tracing` logs with transaction context.
Behaviour is verified by new `rstest` unit tests and `rstest-bdd` scenarios,
and the updated behaviour is documented in `docs/design.md` and, if
user-visible, in `docs/users-guide.md`. The roadmap entry 1.4.5 is marked done
after implementation and verification.

## Constraints

- Domain modules must not import `wireframe::*`; only adapter modules under
  `src/wireframe` and `src/server/wireframe.rs` may depend on wireframe.
- Avoid new external dependencies. If a new crate is required, stop and
  escalate.
- Every new module must start with a module-level `//!` comment.
- All new public APIs require Rustdoc comments with examples.
- Keep files under 400 lines; split new helpers across modules if needed.
- Use en-GB-oxendict spelling in documentation and wrap prose at 80 columns.
- Commit after each logical change and run quality gates before committing.
- Run `make check-fmt`, `make lint`, and `make test` before each commit. If
  documentation changes, also run `make fmt`, `make markdownlint`, and
  `make nixie`.
- Use `rstest` for unit tests and `rstest-bdd` v0.3.2 for behavioural tests.
- For Postgres-backed tests, use `pg_embedded_setup_unpriv` as documented in
  `docs/pg-embedded-setup-unpriv-users-guide.md`.

## Tolerances (Exception Triggers)

- Scope: if more than 10 files change or net changes exceed 400 LOC, stop and
  escalate.
- Interface: if a public API outside `src/wireframe` or `src/server` must
  change, stop and escalate.
- Dependencies: if a new external dependency is required, stop and escalate.
- Iterations: if tests or lint still fail after two fix attempts, stop and
  escalate.
- Ambiguity: if multiple valid error-code mappings exist that materially affect
  client behaviour, stop and present options with trade-offs.

## Risks

- Risk: the desired error-code mapping for malformed payloads is ambiguous
  between `ERR_INVALID_PAYLOAD` and `ERR_INTERNAL_SERVER`. Severity: medium
  Likelihood: medium Mitigation: cross-check `docs/protocol.md`, legacy handler
  behaviour, and existing tests; document the chosen mapping in
  `docs/design.md` and the Decision Log.

- Risk: preserving transaction IDs for parse failures may require parsing the
  header separately, which could diverge from current error handling. Severity:
  low Likelihood: medium Mitigation: add targeted unit tests for malformed
  payloads that still contain a valid header to prove ID retention.

- Risk: logging changes may be difficult to assert in tests without adding new
  dependencies. Severity: low Likelihood: medium Mitigation: keep log changes
  minimal and consistent with existing `tracing` usage; avoid new crates and
  focus tests on reply behaviour.

## Progress

    - [x] (2026-01-17 18:40Z) Reviewed routing error paths, reply helpers, and
      legacy conventions to scope the reply builder.
    - [x] (2026-01-17 18:55Z) Implemented the reply builder and integrated it
      with routing error paths to preserve transaction IDs.
    - [x] (2026-01-17 19:10Z) Added unit and BDD coverage, updated
          documentation,
      and marked the roadmap entry as done.
    - [x] (2026-01-17 19:30Z) Ran formatting, lint, test, and documentation
      checks; captured results.
    - [x] (2026-01-18 01:05Z) Added a log-capture test harness for tracing
      fields, added coverage for missing-reply logging, and reviewed remaining
      test duplication (no further refactors required).

## Surprises & Discoveries

- None.

## Decision Log

    - Decision: Keep `ERR_INTERNAL_SERVER` for routing parse and command
      failures rather than mapping to `ERR_INVALID_PAYLOAD`.
      Rationale: `docs/design.md` already specifies internal errors for parse
      failures, and existing tests assert error code 3 for malformed inputs.
      Date/Author: 2026-01-17 / Codex.

## Outcomes & Retrospective

Error replies now preserve transaction IDs/types when the header is available,
and routing failures log transaction context through `tracing`. Unit and BDD
coverage exercise truncated payloads, unknown transactions, and reply
construction paths. Documentation and roadmap entries were updated to reflect
the change.

## Context and Orientation

Relevant areas to review before editing:

- `src/wireframe/routes/mod.rs` contains routing error handlers
  (`handle_parse_error`, `handle_command_parse_error`, `handle_process_error`,
  `handle_missing_reply`) and will likely host or call the reply builder.
- `src/commands/mod.rs` and `src/commands/handlers.rs` define error codes and
  handler behaviour. `ERR_INVALID_PAYLOAD` and `ERR_INTERNAL_SERVER` are
  defined here.
- `src/header_util.rs` provides `reply_header`, which mirrors request headers.
- `src/transaction/frame.rs` and `src/transaction/errors.rs` define header
  parsing and `TransactionError` variants that may drive error-code mapping.
- `src/news_handlers/mod.rs` shows existing error logging conventions (e.g.
  `error!(%err, context, "news handler error")`) that new logging should mirror.
- `tests/features/wireframe_routing.feature` and
  `tests/wireframe_routing_bdd.rs` cover current routing behaviour with
  `rstest-bdd` v0.3.2.
- `src/wireframe/routes/tests/error_cases.rs` includes unit tests for error
  replies and is the primary location for new `rstest` cases.
- Design and testing guidance lives in `docs/design.md`, `docs/protocol.md`,
  `docs/verification-strategy.md`, `docs/rust-testing-with-rstest-fixtures.md`,
  `docs/reliable-testing-in-rust-via-dependency-injection.md`,
  `docs/rstest-bdd-users-guide.md`, and
  `docs/pg-embedded-setup-unpriv-users-guide.md`.

Terminology used in this plan:

- Reply builder: a helper that constructs Hotline reply transactions for error
  cases, preserving IDs and logging with `tracing`.
- Error propagation: mapping internal failures to Hotline error codes and
  emitting a reply without crashing the connection.

## Plan of Work

Stage A: Confirm current error handling and conventions (no code changes).

Inspect routing error helpers in `src/wireframe/routes/mod.rs`, existing error
code mappings in `src/commands/mod.rs`, and logging conventions in
`src/news_handlers/mod.rs` and `src/login.rs`. Review `docs/protocol.md` for
reply semantics and `docs/design.md` for error handling expectations. Decide
how to map `TransactionError` to Hotline error codes (e.g.
`ERR_INVALID_PAYLOAD` vs `ERR_INTERNAL_SERVER`) and record that decision in
`docs/design.md` and the Decision Log.

Stage B: Implement the reply builder and integrate it.

Introduce a focused reply builder module or helper in the wireframe routing
layer (for example `src/wireframe/routes/reply_builder.rs`), ensuring it has a
module doc comment. The builder should:

- Accept raw frame bytes and/or a `FrameHeader` plus `SocketAddr` to provide
  logging context.
- Attempt to parse the header when at least `HEADER_LEN` bytes are available so
  error replies retain the original transaction ID and type.
- Produce an error `Transaction` using `reply_header` with `is_reply = 1`, the
  mapped error code, and an empty payload.
- Emit structured `tracing` logs at the same levels used today (warn for parse
  or command decode errors; error for processing failures or missing replies),
  and include fields such as `peer`, `ty`, `id`, and `error_code`.

Wire the builder into `process_transaction_bytes` so all routing error paths
use it instead of ad-hoc helpers. Keep error reply construction in one place
and keep the existing behaviour for successful replies unchanged.

Stage C: Tests and documentation.

Add `rstest` unit tests in `src/wireframe/routes/tests/error_cases.rs` to cover:

- Parse failures with a valid header still preserve `id` and `ty` in the reply.
- Malformed payloads map to the selected error code.
- Truncated frames (shorter than `HEADER_LEN`) still return a valid error reply
  with `is_reply = 1` and an empty payload.

Add or extend `rstest-bdd` scenarios in
`tests/features/wireframe_routing.feature` to validate that error replies
preserve transaction IDs for malformed payloads (not just unknown types). Use
existing step definitions in `tests/wireframe_routing_bdd.rs` or add new ones
if needed.

Update documentation:

- `docs/design.md`: document the reply builder, error-code mapping, and logging
  behaviour, including how the header is recovered for error replies.
- `docs/users-guide.md`: update only if the user-visible behaviour changes
  (e.g., error replies become deterministic for malformed payloads). If no
  user-visible change, explicitly note that no update is required.
- `docs/roadmap.md`: mark 1.4.5 as done with date and brief status summary.

Stage D: Verification and commits.

Run formatting, lint, and tests per the Makefile targets. Gate each commit with
`make check-fmt`, `make lint`, and `make test`. If documentation changes, also
run `make fmt`, `make markdownlint`, and `make nixie`. Capture outputs with
`tee` logs and summarise results in the Decision Log if any issues arise.

## Concrete Steps

All commands run from the repository root. Pipe long outputs to a log using
`tee` with `/tmp/$ACTION-$(get-project)-$(git branch --show).out`. If the
`get-project` helper is unavailable, substitute `$(basename "$PWD")`.

1) Discovery

    rg -n "handle_parse_error|handle_process_error|error_transaction" \
        src/wireframe/routes/mod.rs
    rg -n "ERR_INVALID_PAYLOAD|ERR_INTERNAL_SERVER" src/commands/mod.rs
    rg -n "reply_header" src/header_util.rs
    rg -n "tracing::(warn|error|info)" src/login.rs src/news_handlers/mod.rs
    rg -n "wireframe_routing" tests/features/wireframe_routing.feature \
        tests/wireframe_routing_bdd.rs

2) Implement reply builder and routing integration

    $EDITOR src/wireframe/routes/mod.rs
    $EDITOR src/wireframe/routes/reply_builder.rs (if introducing a new module)

3) Tests

    $EDITOR src/wireframe/routes/tests/error_cases.rs
    $EDITOR tests/features/wireframe_routing.feature
    $EDITOR tests/wireframe_routing_bdd.rs

4) Documentation and roadmap

    $EDITOR docs/design.md
    $EDITOR docs/users-guide.md
    $EDITOR docs/roadmap.md

5) Formatting and verification

    make fmt | tee /tmp/fmt-$(get-project)-$(git branch --show).out
    make markdownlint | tee \
        /tmp/markdownlint-$(get-project)-$(git branch --show).out
    make nixie | tee /tmp/nixie-$(get-project)-$(git branch --show).out
    make check-fmt | tee /tmp/check-fmt-$(get-project)-$(git branch --show).out
    make lint | tee /tmp/lint-$(get-project)-$(git branch --show).out
    make test | tee /tmp/test-$(get-project)-$(git branch --show).out

## Validation and Acceptance

Acceptance is met when:

- Error replies preserve transaction IDs and types whenever a valid header is
  present (validated by new unit and BDD tests).
- Routing error paths emit structured `tracing` logs using existing logging
  levels (warn for parse/decode errors, error for processing failures).
- `make check-fmt`, `make lint`, and `make test` pass.
- If documentation changes, `make fmt`, `make markdownlint`, and `make nixie`
  pass.
- `docs/design.md` reflects the reply builder decision and
  `docs/roadmap.md` marks 1.4.5 as done.

## Idempotence and Recovery

Edits are safe to re-run. If a test fails, fix the issue and re-run the
relevant command. If documentation linting fails, run `make fmt` and re-run
`make markdownlint` and `make nixie`.

## Artifacts and Notes

Keep the `tee` logs listed above. If any command fails, note the failure and
resolution in the Decision Log.

## Interfaces and Dependencies

No new external dependencies are expected. If a new module is added, keep it
within `src/wireframe/routes/` and document it with a module-level `//!`
comment.

Proposed helper surface (adjust if the codebase requires different naming):

    pub struct ReplyBuilder {
        header: Option<FrameHeader>,
        peer: SocketAddr,
    }

    impl ReplyBuilder {
        pub fn from_frame(frame: &[u8], peer: SocketAddr) -> Self { … }
        pub fn from_header(header: FrameHeader, peer: SocketAddr) -> Self { … }
        pub fn error_reply(&self, error_code: u32) -> Transaction { … }
    }

`ReplyBuilder` should encapsulate header recovery, error-code mapping, and
logging in one place so routing error paths all behave consistently.

## Revision note (required when editing an ExecPlan)

Updated the status to COMPLETE, filled in progress timestamps, recorded the
error-code mapping decision, and summarised outcomes now that the reply builder
implementation and tests are in place.

Revision: 2026-01-18

- Status set to IN PROGRESS to cover follow-up test harness work.

Revision: 2026-01-18

- Log-capture harness complete; status returned to COMPLETE.
