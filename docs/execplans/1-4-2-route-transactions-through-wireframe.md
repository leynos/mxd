# Route transactions through wireframe (Task 1.4.2)

This ExecPlan is a living document. The sections `Progress`,
`Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must
be kept up to date as work proceeds.

Status: COMPLETE

No `PLANS.md` exists in this repository.

## Purpose / Big Picture

After this change, the wireframe server routes incoming Hotline transactions
(Login, file listing, news listing) to domain handlers without code
duplication. Integration tests run against the wireframe server exclusively,
validating that the migration from the legacy networking stack is complete for
these transaction types.

Observable outcome: running `make test` exercises Login, news listing, and file
listing through the wireframe transport, with all tests passing. The
`legacy-networking` feature gate is no longer required for these integration
tests.

## Constraints

- Files must stay under 400 lines; every Rust module starts with a `//!`
  module-level comment.
- Routing behaviour for Login, file listing, and news listing must not change.
- Validation must use Makefile targets (`make check-fmt`, `make lint`,
  `make test`, `make markdownlint`).
- Documentation uses en-GB-oxendict spelling and wraps paragraphs at 80
  columns.
- No new dependencies without explicit approval.

## Tolerances (Exception Triggers)

- Scope: if more than 20 files or 600 net lines are required, stop and
  escalate.
- Interfaces: if public APIs in `src/server/wireframe.rs` or
  `src/wireframe/routes/mod.rs` must change, stop and escalate.
- Dependencies: if any new crate is required, stop and escalate.
- Tests: if tests fail after two iterations of fixes, stop and escalate.
- Ambiguity: if multiple valid interpretations of routing behaviour exist,
  stop and present options with trade-offs.

## Risks

- Risk: Frame codec framing could diverge from Hotline header expectations.
  Severity: high. Likelihood: low. Mitigation: keep the codec backed by the
  shared transaction parser and verify framing in unit tests.
- Risk: Behavioural tests depend on the embedded Postgres harness and may be
  flaky under load. Severity: medium. Likelihood: medium. Mitigation: keep
  timeouts explicit and avoid shared state across scenarios.
- Risk: Unknown route IDs might bypass error reporting. Severity: medium.
  Likelihood: low. Mitigation: maintain fallback route registration and assert
  ERR_INTERNAL in tests.

## Progress

- [x] (2025-12-29) Create `docs/execplans/` directory and write this ExecPlan.
- [x] (2025-12-29) Implement `TransactionMiddleware` struct in
      `src/wireframe/routes/mod.rs` that properly wraps `HandlerService`.
- [x] (2025-12-29) Update `build_app()` in `src/server/wireframe.rs` to use
      `TransactionMiddleware` instead of `from_fn` (which had type mismatch).
- [x] (2025-12-29) Refactor middleware to pass `DbPool` and `Session` directly
      (not via thread-locals) to work with Tokio's work-stealing scheduler.
- [x] (2025-12-29) Update `TestServer` in `test-util/src/server.rs` to launch
      `mxd-wireframe-server` binary; all postgres tests pass.
- [x] (2026-01-04) Switch the wireframe server to `WireframeServer` with the
      `HotlineFrameCodec`, removing the bespoke accept loop and connection
      handler.
- [x] (2026-01-04) Remove `#[cfg(feature = "legacy-networking")]` gates from
      wireframe integration tests (file list/news listings).
- [x] (2026-01-04) Add rstest unit tests for route handlers, split into
      `src/wireframe/routes/tests/` to keep files under 400 lines while
      covering Login/File/News transaction routing.
- [x] (2026-01-04) Add rstest-bdd behavioural tests in
      `tests/wireframe_routing_bdd.rs` and extend
      `tests/features/wireframe_routing.feature` with login/file/news
      scenarios.
- [x] (2026-01-04) Ensure Postgres-backed routing scenarios use the embedded
      Postgres harness (`pg-embedded-setup-unpriv`) via `PostgresTestDb`.
- [x] (2026-01-04) Update `docs/design.md` with routing architecture and
      frame codec details.
- [x] (2026-01-04) Mark task 1.4.2 as done in `docs/roadmap.md`.
- [x] (2026-01-04) Confirm `HotlineFrameCodec` is wired into
      `src/server/wireframe.rs` and replaces the bespoke Tokio codec.
- [x] (2026-01-05) Consolidate connection-scoped thread-local state into a
      `ConnectionContext` and remove unused pool/session helpers.
- [x] (2026-01-05) Simplify transaction routing middleware to avoid generic
      service plumbing and frame copies.
- [x] (2026-01-05) Remove unused `SessionState` wrapper and align tests with
      the production session handling.
- [x] (2026-01-05) Gate `error_reply` to test builds and align unknown-type
      error codes with ERR_INTERNAL.
- [x] (2026-01-05) Bound readiness diagnostics in the test server harness and
      surface stdout flush failures.
- [x] (2026-01-05) Require handshake context/peer in the app factory and log
      peer address lookup failures.
- [x] (2026-01-05) Add dispatch spy coverage for middleware routing and
      simplify test server binary resolution for integration runs.
- [x] (2026-01-05) Harden middleware routing spy assertions to ignore
      unrelated dispatches during coverage runs.
- [x] (2026-01-05) Extract shared routing test helpers into
      `wireframe::test_helpers` and re-export them via `test-util` so unit and
      BDD scenarios share one implementation without type mismatches.
- [x] (2026-01-05) Validate reassembled transactions in `HotlineTransaction`
      and guard framed decoding against oversized fragments.
- [x] (2026-01-05) Serialize the middleware routing test and remove redundant
      drops in wireframe server BDD helpers.
- [x] (2026-01-05) Refactor `HotlineTransaction::from_parts` conditionals to
      keep codec validation readable and within complexity thresholds.
- [x] (2026-01-05) Address PR review feedback by tightening codec tests,
      adding build-frame unit coverage, and aligning documentation grammar
      with Oxford spelling conventions.
- [x] (2026-01-06) Address follow-up review notes by splitting Hotline codec
      tests into `framed_tests.rs`, returning `Result` from routing helpers,
      and enforcing sqlite/postgres exclusivity in `test-util`.
- [x] (2026-01-06) Reformat key file notes as a definition list and align
      documentation language with adaptor terminology guidance.

## Surprises & Discoveries

- Observation: wireframe v0.1.0's `from_fn` middleware helper cannot be used
  with `WireframeApp::wrap()` due to a type mismatch. Evidence: `from_fn`
  produces `FnService<HandlerService<E>, F>` but `Middleware<E>` requires
  `Transform<HandlerService<E>, Output = HandlerService<E>>`. Resolution:
  Implemented custom `TransactionMiddleware` struct that implements `Transform`
  directly and wraps output in `HandlerService::from_service()`.

- Observation: Thread-local storage does not work with Tokio's work-stealing
  scheduler for passing connection state between `build_app()` and middleware.
  Evidence: Tests returned ERR_INTERNAL (3) when thread-locals stored the pool
  because middleware could run on a different thread than `build_app()`.
  Resolution: Store `DbPool` and `Session` directly in `TransactionMiddleware`
  struct fields; the middleware receives them in `new()` and clones into the
  inner `TransactionService` during `transform()`.

- Observation: wireframe v0.2.0 adds `FrameCodec` support, allowing custom
  framing to be installed with `WireframeApp::with_codec`. The worked example
  in `../wireframe/examples/hotline_codec.rs` shows a `HotlineFrameCodec`
  implementation that wraps the 20-byte header framing. This means we can use
  wireframe's built-in connection handling again and retire the custom
  Tokio-based codec and accept loop.

- Observation: wireframe routing requires a registered handler for each route
  ID, so unknown Hotline transaction types must be mapped to a fallback route
  when using `FrameCodec`. Resolution: introduce route ID helpers and a
  fallback route (0) so middleware can still emit error replies for unknown
  transactions.

- Observation: the routing test module exceeded the 400-line file limit once
  the per-transaction success cases were added. Resolution: split the tests
  into `src/wireframe/routes/tests/error_cases.rs`,
  `src/wireframe/routes/tests/routing_cases.rs`, and shared helpers.

- Observation: the `self_named_module_files` lint rejects `routes.rs` once
  submodules live under `src/wireframe/routes/`. Resolution: move the module
  file to `src/wireframe/routes/mod.rs` so linting passes with nested tests.

- Observation: routing BDD and unit tests duplicated database, frame, and
  parameter helper logic, and sharing helpers through `test-util` caused type
  mismatches due to duplicate `mxd` crates. Resolution: centralize
  `build_frame`/`collect_strings` in `wireframe::test_helpers` and re-export
  them from `test-util` while keeping DB setup in `test-util`.

## Decision Log

- Decision: Tests run against wireframe server only.
  Rationale: Simplifies test maintenance; legacy networking is deprecated.
  Date/Author: 2025-12-29 / User decision.

- Decision: Replace the bespoke Tokio `HotlineCodec` and manual accept loop
  with wireframe's `FrameCodec` integration, using
  `wireframe::codec::examples::HotlineFrameCodec` (or an equivalent in-tree
  implementation) via `WireframeApp::with_codec`. Rationale: Wireframe v0.2.0
  now supports custom frame codecs, so the 20-byte Hotline header can be
  handled within wireframe's standard connection pipeline, keeping routing and
  middleware intact without custom TCP plumbing. Date/Author: 2025-12-30 /
  User-driven investigation.

- Decision: Implement an in-tree `HotlineFrameCodec` that wraps bincode
  `Envelope` payloads and map unknown transaction types to fallback route ID
  0. Rationale: The wireframe server needs a handler for each route ID; mapping
  unknown types to a known fallback keeps middleware routing in place, so error
  responses can be generated consistently. Date/Author: 2026-01-04 / Assistant
  implementation.

- Decision: Move routing BDD steps into `tests/wireframe_routing_bdd.rs` and
  split routing unit tests into submodules under `src/wireframe/routes/tests/`.
  Rationale: Keep behavioural tests in the integration suite and satisfy the
  400-line file size requirement while adding per-transaction routing coverage.
  Date/Author: 2026-01-04 / Assistant implementation.

- Decision: Host shared frame/parameter helpers in
  `wireframe::test_helpers` and re-export them from `test-util`. Rationale:
  Sharing helpers through `test-util` alone introduced duplicate `mxd` crate
  types in unit tests; placing helpers in the main crate keeps types aligned
  while still de-duplicating logic. Date/Author: 2026-01-05 / Assistant
  implementation.

- Decision: Migrate `src/wireframe/routes.rs` to
  `src/wireframe/routes/mod.rs`. Rationale: Required by the
  `self_named_module_files` lint once the routing tests were split into
  submodules. Date/Author: 2026-01-04 / Assistant implementation.

- Decision: Unknown transaction types return ERR_INTERNAL (code 3) with warning
  log. Rationale: Aligns routing behaviour with the postmortem spec and avoids
  ambiguous error codes. Date/Author: 2025-12-29 / User decision.

- Decision: Implement custom `HotlineCodec` bypassing wireframe's routing.
  Rationale: wireframe v0.1.0 hardcodes `LengthDelimitedCodec` with 4-byte
  length prefix framing at `src/app/connection.rs:47`. Hotline uses 20-byte
  header framing, which is incompatible. No extension point exists in wireframe
  for custom codecs. The custom codec implements Tokio's `Decoder`/`Encoder`
  traits directly, enabling proper transaction framing while still using
  wireframe for preamble (handshake) handling. Status: Superseded by the
  2025-12-30 decision to use `FrameCodec`. Date/Author: 2025-12-29 /
  Implementation discovery.

- Decision: Use custom TCP accept loop instead of `WireframeServer::run()`.
  Rationale: wireframe's `handle_connection()` function wraps the stream with
  `LengthDelimitedCodec` internally, which cannot be overridden. To use our
  custom `HotlineCodec`, we must bypass wireframe's connection handling
  entirely. The server used: (1) wireframe's `read_preamble()` for handshake
  parsing, (2) a bespoke connection handler with
  `Framed<RewindStream, HotlineCodec>` for transaction processing. Status:
  Superseded by the 2025-12-30 decision to use `FrameCodec`. Date/Author:
  2025-12-29 / Implementation discovery.

- Decision: Name codec module `framed.rs` instead of `tokio.rs`. Rationale:
  Avoid name collision with the `tokio` crate, which would cause confusing
  import paths and potential shadowing issues. The name `framed` reflects the
  module's purpose (providing `Framed`-compatible codec) without conflicting
  with external crate names. Status: Superseded by the 2025-12-30 decision to
  use `FrameCodec`. Date/Author: 2025-12-29 / Implementation decision.

- Decision: Consolidate connection-scoped thread-local data into
  `ConnectionContext`, removing unused pool/session storage helpers. Rationale:
  The middleware now carries pool/session directly, making the extra thread
  locals dead code; a single context reduces helper count and simplifies
  access. Date/Author: 2026-01-05 / Assistant implementation.

- Decision: Use a concrete `TransactionHandler` and pass frame slices directly
  into `process_transaction_bytes` to avoid per-request copies. Rationale:
  Reduces allocation and simplifies middleware internals while preserving
  handler pipeline behaviour. Date/Author: 2026-01-05 / Assistant
  implementation.

- Decision: Treat missing handshake context/peer or app-construction failures
  as fatal for the connection. Rationale: Running without peer metadata or
  routes would hide faults and silently break routing, so connections should
  fail fast instead. Date/Author: 2026-01-06 / Assistant implementation.

## Outcomes & Retrospective

- Routing tests cover error handling and successful Login/File/News routing
  paths for each supported transaction type.
- Behavioural scenarios now include login, file listing, and news listing
  coverage in the Gherkin feature file with dedicated step definitions.
- Documentation and roadmap entries are updated to reflect the wireframe codec
  integration and routing test coverage.
- Wireframe's `FrameCodec` integration now handles Hotline framing without the
  bespoke Tokio accept loop.
- Connection-scoped state uses a single thread-local context, and transaction
  middleware avoids redundant frame copies.
- The routing layer no longer carries unused `RouteState` or `SessionState`
  wrappers.
- Routing test helpers now live in `wireframe::test_helpers` and are
  re-exported from `test-util`, reducing duplication between unit and
  behavioural suites.
- Framed decoding validates fragment sizing before building a transaction.

## Context and Orientation

The mxd codebase implements a Hotline-compatible server using hexagonal
architecture. The transport layer is migrating from a bespoke TCP loop
(`src/server/legacy/`) to the `wireframe` library.

Key files and their roles:

`src/server/wireframe.rs`: Builds a `WireframeServer` with `HotlineFrameCodec`,
registers supported route IDs (plus the fallback route), and installs
`TransactionMiddleware` for transaction processing.

`src/wireframe/routes/mod.rs`: Contains `process_transaction_bytes()` which
parses raw bytes, dispatches to `Command::process()`, and returns reply bytes.
This function already implements the domain routing logic.

`src/wireframe/codec/frame.rs`: `HotlineFrameCodec` maps Hotline transactions
into bincode `Envelope` payloads for wireframe routing.

`src/wireframe/codec/framed.rs`: Tokio `HotlineCodec` used by the frame codec
to decode and encode the 20-byte Hotline headers.

`src/wireframe/route_ids.rs`: Route ID mapping (including the fallback handler).

`src/wireframe/protocol.rs`: `HotlineProtocol` implements `WireframeProtocol`
with lifecycle hooks (`on_connection_setup`, `before_send`, etc.).

`src/commands.rs`: `Command` enum with variants for each transaction type
(Login, GetFileNameList, etc.) and the `process()` method that executes
handlers.

`tests/file_list.rs`: Integration tests that exercise the wireframe server
(legacy networking gate removed).

`tests/news_categories.rs`: Integration tests that exercise the wireframe
server (legacy networking gate removed).

`test-util/src/server.rs`: `TestServer` harness that starts the server binary
for integration tests.

Transaction type IDs (from `src/transaction_type.rs`):

- 107 = Login
- 200 = GetFileNameList
- 370 = NewsCategoryNameList
- 371 = NewsArticleNameList
- 400 = GetNewsArticleData
- 410 = PostNewsArticle

The wireframe library's `WireframeApp` provides `.route(id, handler)` for
registering handlers by message identifier.

## Plan of Work

The implementation proceeds in four phases:

### Phase 1: Route registration infrastructure

In `src/server/wireframe.rs`, register no-op handlers for each implemented
transaction type (107, 200, 370, 371, 400, 410) plus a fallback route (0).
Routing remains in `TransactionMiddleware`, which processes raw transaction
bytes and emits replies.

Use `src/wireframe/route_ids.rs` helpers to map transaction types to route IDs
and keep the fallback mapping in one place.

### Phase 1.5: Replace bespoke framing with `FrameCodec`

Replace the Tokio `HotlineCodec` and custom accept loop with wireframe's
`FrameCodec` support. Use `WireframeApp::with_codec` and a local
`HotlineFrameCodec` implementation that matches the 20-byte Hotline header.
Remove the bespoke connection handler once routing is handled by
`WireframeServer::run()`.

### Phase 2: Test infrastructure migration

In `test-util/src/server.rs`, update `TestServer` to start the wireframe server
binary (`mxd-wireframe-server`) instead of the legacy server. Ensure the
handshake helper works with the wireframe preamble.

Remove `#[cfg(feature = "legacy-networking")]` gates from `tests/file_list.rs`
and `tests/news_categories.rs`. The tests should now exercise the wireframe
server.

### Phase 3: Testing

Add unit tests in `src/wireframe/routes/tests/` using rstest:

- Parse valid transactions correctly.
- Return an error for malformed input.
- Preserve the transaction ID in the reply.
- Unknown transaction type returns ERR_INTERNAL.

Add behavioural tests using rstest-bdd v0.3.0:

- Create `tests/features/wireframe_routing.feature` with scenarios for Login,
  file listing, and news listing.
- Implement step definitions in `tests/wireframe_routing_bdd.rs`.
- Add Postgres-backed scenarios using `pg-embedded-setup-unpriv`.

### Phase 4: Documentation

Update `docs/design.md` with the routing architecture. Update
`docs/users-guide.md` if any user-facing behaviour changes. Mark task 1.4.2 as
done in `docs/roadmap.md`.

## Concrete Steps

All commands run from the repository root `<repo-root>`.

1. Create the execplans directory and write this file:

       mkdir -p docs/execplans
       # Write this ExecPlan to docs/execplans/1-4-2-route-transactions-through-wireframe.md

2. After implementing route registration, verify compilation:

       cargo build --features wireframe

   Expected: Build succeeds with no errors.

3. Run the test suite (use `tee` to capture full output):

       set -o pipefail
       make test | tee /tmp/mxd-make-test.log

   Expected: All tests pass. Login, file listing, and news listing tests
   exercise the wireframe server.

4. Run linting and formatting checks:

       set -o pipefail
       make check-fmt | tee /tmp/mxd-make-check-fmt.log
       make lint | tee /tmp/mxd-make-lint.log

   Expected: No warnings or errors.

5. Run Markdown validation:

       set -o pipefail
       make markdownlint | tee /tmp/mxd-make-markdownlint.log

   Expected: All documentation passes linting.

## Validation and Acceptance

Acceptance criteria:

1. `make test` passes with all integration tests running against the wireframe
   server.

2. The following test files no longer have `#[cfg(feature =
   "legacy-networking")]` gates:
   - `tests/file_list.rs`
   - `tests/news_categories.rs`

3. New unit tests exist in `src/wireframe/routes/tests/` covering:
   - Valid transaction routing for each type (107, 200, 370, 371, 400, 410).
   - Error handling for unknown transaction types.

4. New behavioural tests exist in `tests/features/wireframe_routing.feature`
   with step definitions in `tests/wireframe_routing_bdd.rs`.

5. Task 1.4.2 is marked as done in `docs/roadmap.md`.

6. `make check-fmt`, `make lint`, and `make markdownlint` all pass.

## Idempotence and Recovery

All steps are safe to repeat. If a step fails partway through, re-run the
failing phase after correcting the error. Avoid destructive commands unless
explicitly approved.

Commits should be atomic and focused. If a commit introduces a regression, it
can be reverted independently.

## Artifacts and Notes

Example route registration pattern (from `docs/wireframe-users-guide.md`):

    WireframeApp::new()?
        .route(1, handler)?
        .route(2, handler)?

Example handler signature from wireframe library:

    async fn handler(env: &Envelope) {
        // Extract frame from envelope
        // Dispatch to domain logic
    }

## Interfaces and Dependencies

In `src/server/wireframe.rs`, `build_app()` registers the fallback route and
the supported route IDs after wiring the transaction middleware.

Dependencies:

- `wireframe` crate (already in use)
- `rstest` v0.26 (already in dev-dependencies)
- `rstest-bdd` v0.3.0
- `pg-embedded-setup-unpriv` (already in dev-dependencies)

## Revision Note

Updated the ExecPlan to include status, constraints, tolerances, and risks;
documented the FrameCodec confirmation; refreshed validation commands to use
`tee`; aligned paths with the current workspace root; corrected the unknown
type error code; documented the connection context consolidation and middleware
simplification; recorded readiness diagnostics and app factory failure
handling; removed the unused `RouteState` and `SessionState` wrappers;
clarified the rstest-bdd version to match Cargo.toml; noted the shared routing
test helpers in `wireframe::test_helpers` (re-exported via `test-util`);
tightened framed decoding validation; and updated the interface summary to
match the current route registration flow. This does not change remaining work
because the task is complete.
