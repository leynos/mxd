# Route transactions through wireframe (Task 1.4.2)

This ExecPlan is a living document. The sections `Progress`,
`Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must
be kept up to date as work proceeds.

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

## Progress

- [x] (2025-12-29) Create `docs/execplans/` directory and write this ExecPlan.
- [x] (2025-12-29) Implement `TransactionMiddleware` struct in
      `src/wireframe/routes.rs` that properly wraps `HandlerService`.
- [x] (2025-12-29) Update `build_app()` in `src/server/wireframe.rs` to use
      `TransactionMiddleware` instead of `from_fn` (which had type mismatch).
- [x] (2025-12-29) Refactor middleware to pass `DbPool` and `Session` directly
      (not via thread-locals) to work with Tokio's work-stealing scheduler.
- [x] (2025-12-29) Update `TestServer` in `test-util/src/server.rs` to launch
      `mxd-wireframe-server` binary; all postgres tests pass.
- [ ] Remove `#[cfg(feature = "legacy-networking")]` gates from integration
      tests.
- [ ] Add rstest unit tests for route handlers.
- [ ] Add rstest-bdd behavioural tests with feature file.
- [ ] Add Postgres-backed tests using pg-embedded-setup-unpriv.
- [ ] Update `docs/design.md` with routing architecture.
- [ ] Mark task 1.4.2 as done in `docs/roadmap.md`.

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

## Decision Log

- Decision: Tests run against wireframe server only.
  Rationale: Simplifies test maintenance; legacy networking is deprecated.
  Date/Author: 2025-12-29 / User decision.

- Decision: Unknown transaction types return ERR_INTERNAL (code 3) with warning
  log. Rationale: Consistent with existing error handling in `commands.rs`;
  provides visibility into unsupported requests. Date/Author: 2025-12-29 / User
  decision.

## Outcomes & Retrospective

(To be completed upon finishing implementation.)

## Context and Orientation

The mxd codebase implements a Hotline-compatible server using hexagonal
architecture. The transport layer is migrating from a bespoke TCP loop
(`src/server/legacy/`) to the `wireframe` library.

Key files and their roles:

- `src/server/wireframe.rs` (lines 104-125): `build_app()` creates a
  `WireframeApp` with the `HotlineProtocol` adapter via `.with_protocol()`. It
  currently attaches app data (config, pool, argon2, handshake) but does not
  register routes.

- `src/wireframe/routes.rs`: Contains `RouteState`, `SessionState`, and
  `process_transaction_bytes()` which parses raw bytes, dispatches to
  `Command::process()`, and returns reply bytes. This function already
  implements the domain routing logic.

- `src/wireframe/protocol.rs`: `HotlineProtocol` implements `WireframeProtocol`
  with lifecycle hooks (`on_connection_setup`, `before_send`, etc.).

- `src/commands.rs`: `Command` enum with variants for each transaction type
  (Login, GetFileNameList, etc.) and the `process()` method that executes
  handlers.

- `tests/file_list.rs`, `tests/news_categories.rs`: Integration tests currently
  gated behind `#[cfg(feature = "legacy-networking")]`.

- `test-util/src/server.rs`: `TestServer` harness that starts the server binary
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

In `src/wireframe/routes.rs`, add a route handler function that wraps
`process_transaction_bytes`. The handler extracts the frame bytes, peer
address, database pool, and session state from wireframe's app data and
envelope, then delegates to the existing processing logic.

Add a `register_routes()` function that chains `.route()` calls for each
implemented transaction type (107, 200, 370, 371, 400, 410). Add a fallback
route or error handler for unknown types that returns `ERR_INTERNAL` with a
warning log.

In `src/server/wireframe.rs`, update `build_app()` to call `register_routes()`
after registering the protocol adapter.

### Phase 2: Test infrastructure migration

In `test-util/src/server.rs`, update `TestServer` to start the wireframe server
binary (`mxd-wireframe-server`) instead of the legacy server. Ensure the
handshake helper works with the wireframe preamble.

Remove `#[cfg(feature = "legacy-networking")]` gates from `tests/file_list.rs`
and `tests/news_categories.rs`. The tests should now exercise the wireframe
server.

### Phase 3: Testing

Add unit tests in `src/wireframe/routes.rs` using rstest:

- Handler correctly parses valid transactions.
- Handler returns error for malformed input.
- Handler preserves transaction ID in reply.
- Unknown transaction type returns ERR_INTERNAL.

Add behavioural tests using rstest-bdd v0.3.2:

- Create `tests/features/wireframe_routing.feature` with scenarios for Login,
  file listing, and news listing.
- Implement step definitions in `tests/wireframe_routing_bdd.rs`.
- Add Postgres-backed scenarios using `pg-embedded-setup-unpriv`.

### Phase 4: Documentation

Update `docs/design.md` with the routing architecture. Update
`docs/users-guide.md` if any user-facing behaviour changes. Mark task 1.4.2 as
done in `docs/roadmap.md`.

## Concrete Steps

All commands run from the repository root `/mnt/home/leynos/Projects/mxd`.

1. Create the execplans directory and write this file:

       mkdir -p docs/execplans
       # Write this ExecPlan to docs/execplans/1-4-2-route-transactions-through-wireframe.md

2. After implementing route registration, verify compilation:

       cargo build --features wireframe

   Expected: Build succeeds with no errors.

3. Run the test suite:

       make test

   Expected: All tests pass. Login, file listing, and news listing tests
   exercise the wireframe server.

4. Run linting and formatting checks:

       make check-fmt
       make lint

   Expected: No warnings or errors.

5. Run Markdown validation:

       make markdownlint

   Expected: All documentation passes linting.

## Validation and Acceptance

Acceptance criteria:

1. `make test` passes with all integration tests running against the wireframe
   server.

2. The following test files no longer have `#[cfg(feature =
   "legacy-networking")]` gates:
   - `tests/file_list.rs`
   - `tests/news_categories.rs`

3. New unit tests exist in `src/wireframe/routes.rs` covering:
   - Valid transaction routing for each type (107, 200, 370, 371, 400, 410).
   - Error handling for unknown transaction types.

4. New behavioural tests exist in `tests/features/wireframe_routing.feature`
   with step definitions in `tests/wireframe_routing_bdd.rs`.

5. Task 1.4.2 is marked as done in `docs/roadmap.md`.

6. `make check-fmt`, `make lint`, and `make markdownlint` all pass.

## Idempotence and Recovery

All steps are safe to repeat. If a step fails partway through:

- Discard uncommitted changes with `git checkout .`.
- Re-run from the beginning of the failing phase.

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

In `src/wireframe/routes.rs`, the following function will be added:

    pub fn register_routes(app: WireframeApp, pool: DbPool, …) -> WireframeApp

In `src/server/wireframe.rs`, `build_app()` will be updated to call
`register_routes()` after `.with_protocol(protocol)`.

Dependencies:

- `wireframe` crate (already in use)
- `rstest` v0.26 (already in dev-dependencies)
- `rstest-bdd` v0.3.2 (update from v0.3.0)
- `pg-embedded-setup-unpriv` (already in dev-dependencies)
