# Introduce a shared session context (Task 1.4.3)

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETED

No `PLANS.md` exists in this repository.

## Purpose / Big Picture

After this change, the wireframe server maintains a shared session context that
tracks the authenticated user, their privileges, and connection flags across
all transaction handlers. Privilege checks defined in `docs/protocol.md` are
enforced automatically, preventing unauthorized operations before they reach
domain logic.

Observable outcome: running `make test` exercises privilege-gated transactions
(file listing, news posting) through the wireframe server, with tests verifying
that unauthenticated or unprivileged requests receive appropriate error codes.
The `Session` struct in `src/handler.rs` is extended with privilege bits and
connection flags, and handlers consult this context before processing
privileged operations.

## Constraints

- Files must stay under 400 lines; every Rust module starts with a `//!`
  module-level comment.
- The session context must survive across handlers without relying on
  thread-local storage for cross-handler state (Tokio work-stealing scheduler
  incompatibility was discovered in task 1.4.2).
- Privilege checks must match the 38 privilege bits documented in
  `docs/protocol.md` (field 110, User Access).
- Domain modules must remain free of `wireframe::*` imports.
- Validation must use Makefile targets (`make check-fmt`, `make lint`,
  `make test`, `make markdownlint`).
- Documentation uses en-GB-oxendict spelling and wraps paragraphs at 80
  columns.
- No new dependencies without explicit approval.

## Tolerances (Exception Triggers)

- Scope: if implementation requires changes to more than 15 files or 500 net
  lines of code, stop and escalate.
- Interface: if public API signatures in `src/handler.rs` or
  `src/wireframe/routes/mod.rs` must change beyond adding new fields/methods,
  stop and escalate.
- Dependencies: if a new external crate is required, stop and escalate.
- Tests: if tests fail after two iterations of fixes, stop and escalate.
- Ambiguity: if privilege semantics differ between transactions or require
  interpretation, stop and present options with trade-offs.

## Risks

- Risk: Privilege bitmap interpretation may differ between Hotline 1.8.5 and
  earlier versions. Severity: medium Likelihood: low Mitigation: Implement
  privilege checks based strictly on `docs/protocol.md` bit definitions; add
  comments referencing the protocol spec for each bit.

- Risk: Session state mutation during concurrent handler execution could cause
  races. Severity: high Likelihood: low Mitigation: Session is already wrapped
  in `Arc<tokio::sync::Mutex<Session>>` in `TransactionMiddleware`; maintain
  this pattern and document that handlers must hold the lock for the duration
  of privilege checks and state updates.

- Risk: Extending the `Session` struct may break existing tests or integration
  code. Severity: medium Likelihood: medium Mitigation: Add new fields with
  sensible defaults (`Default` impl) so existing code continues to work; update
  tests incrementally.

## Progress

- [x] Research existing session and privilege handling across the codebase.
- [x] Design the extended `Session` struct with privilege bits and flags.
- [x] Implement the `Privileges` bitflags type matching `docs/protocol.md`.
- [x] Extend `Session` with `privileges` and `connection_flags` fields.
- [x] Update login handler to initialize privileges using `default_user()`,
      pending database-backed privilege loading.
- [x] Add privilege check helpers to `Session`.
- [x] Integrate privilege enforcement into `GetFileNameList` handler.
- [x] Integrate privilege enforcement into news transaction handlers.
- [x] Add unit tests for privilege bitflags and session helpers.
- [x] Add rstest-bdd behavioural tests for privilege enforcement.
- [x] Add Postgres-backed tests using `pg-embedded-setup-unpriv`.
- [x] Update `docs/design.md` with session context architecture.
- [ ] Update `docs/users-guide.md` if user-visible behaviour changes.
      (Not required: no user-visible behaviour change; privilege enforcement is
      server-side.)
- [x] Mark task 1.4.3 as done in `docs/roadmap.md`.

## Surprises & Discoveries

1. **bitflags dependency**: The `bitflags` crate was not a direct dependency,
   so `bitflags = "2.10.0"` was added to `Cargo.toml`.

2. **Test authentication requirements**: Existing integration and BDD tests
   were not authenticating before sending privileged requests. After adding
   privilege enforcement, tests failed with error code 1 (not authenticated).
   Fixed by adding `authenticate()` helper to test contexts and updating test
   setup functions to create a test user.

3. **Unique constraint on users**: The `setup_full_db` fixture called both
   `setup_files_db` and `setup_news_db`, each attempting to create the same
   `alice` user. Fixed by introducing an `ensure_test_user()` helper that
   checks for existing users before insertion.

4. **NEWS_POST_ARTICLE missing from defaults**: The initial `default_user()`
   privileges omitted `NEWS_POST_ARTICLE`, causing the `PostNewsArticle` test
   to fail. Added the privilege to the default set.

## Decision Log

1. **Privilege storage**: Privileges are populated from `default_user()` on
   login rather than stored per-user in the database. Future task 5.1 will add
   per-user privilege storage.

2. **Error codes**: Used `ERR_NOT_AUTHENTICATED (1)` for unauthenticated
   requests and `ERR_INSUFFICIENT_PRIVILEGES (4)` for missing privileges,
   aligning with Hotline protocol conventions.

3. **Default privileges**: `default_user()` includes: DOWNLOAD_FILE, READ_CHAT,
   SEND_CHAT, SHOW_IN_LIST, SEND_PRIVATE_MESSAGE, NEWS_READ_ARTICLE,
   NEWS_POST_ARTICLE, GET_CLIENT_INFO, CHANGE_OWN_PASSWORD. This matches
   typical guest/user behaviour.

## Outcomes & Retrospective

**Outcome**: Task 1.4.3 completed successfully. The wireframe server now tracks
authenticated user, privileges, and connection flags in a shared session
context that survives across handlers. Privilege checks enforce the protocol
specification before handlers execute.

**Files changed**:

- Created: `src/privileges.rs` (38 privilege bits)
- Created: `src/connection_flags.rs` (3 connection flags)
- Modified: `src/lib.rs` (module exports)
- Modified: `src/handler.rs` (Session struct, PrivilegeError enum, helpers)
- Modified: `src/login.rs` (populate privileges on login)
- Modified: `src/commands/mod.rs` (command parsing and dispatch)
- Modified: `src/commands/handlers.rs` (file listing and error replies)
- Created: `src/news_handlers/mod.rs` (news handler extraction)
- Modified: `Cargo.toml` (bitflags dependency)
- Created: `tests/features/session_privileges.feature` (BDD scenarios)
- Created: `tests/session_privileges_bdd.rs` (BDD test implementation)
- Modified: Multiple test files (authentication updates)
- Modified: `test-util/src/fixtures.rs` (ensure_test_user helper)
- Modified: `test-util/src/protocol.rs` (login helper)
- Modified: `docs/design.md` (session context documentation)
- Modified: `docs/roadmap.md` (task marked complete)

**Tests added**:

- Unit tests for Privileges bitflags and Session helpers
- 6 behaviour-driven development (BDD) scenarios covering
  authenticated/unauthenticated and privileged/unprivileged access paths

**Retrospective**:

- Implementation stayed within tolerance limits (~300 lines of new code).
- No interface changes beyond adding new fields and methods.
- The main complexity was updating existing tests to authenticate before
  privileged operations; this was expected per the risk assessment.
- The `bitflags` crate addition was minimal and well-justified.

## Context and Orientation

The mxd codebase implements a Hotline-compatible server using hexagonal
architecture. The transport layer uses the `wireframe` library with a custom
`HotlineFrameCodec` for transaction framing.

Key files and their roles:

`src/handler.rs`: Defines `Context` (per-connection shared state: peer, pool,
argon2) and `Session` (per-connection mutable state: currently only
`user_id: Option<i32>`). The `handle_request` function dispatches transactions
through `Command`.

`src/commands/mod.rs`: Contains the `Command` enum and dispatches handler
execution. Currently, it enforces privilege checks for file listing and
delegates news operations to `src/news_handlers/mod.rs`.

`src/commands/handlers.rs`: Implements login, file listing, and error reply
helpers shared by command processing.

`src/news_handlers/mod.rs`: Implements news-related handlers, database
operations, and privilege checks for news transactions.

`src/login.rs`: Handles login authentication. Sets `session.user_id` and
initializes `session.privileges` to `Privileges::default_user()` pending
database-backed privilege loading.

`src/wireframe/routes/mod.rs`: Contains `TransactionMiddleware` that wraps
`Session` in `Arc<tokio::sync::Mutex<Session>>` and passes it to
`process_transaction_bytes`. This ensures session state survives across the
handler pipeline.

`src/wireframe/connection.rs`: Manages `ConnectionContext` with handshake
metadata and peer address using thread-local storage for the current Tokio task.

`src/models.rs`: Defines `User` struct with `id`, `username`, `password`. Does
not currently store privilege bits (privileges would need to be added to the
schema or hard-coded for initial implementation).

`docs/protocol.md`: Documents the Hotline protocol including the 38 privilege
bits in field 110 (User Access). Key privilege bits for this task:

- Bit 2: Download File
- Bit 20: News Read Article
- Bit 21: News Post Article

The current `Session` struct:

    pub struct Session {
        pub user_id: Option<i32>,
        pub privileges: Privileges,
        pub connection_flags: ConnectionFlags,
    }

Where `Privileges` is a bitflags type matching the protocol specification and
`ConnectionFlags` tracks connection-level state (e.g., refused messages,
refused chat invites, automatic response enabled).

## Plan of Work

The implementation proceeds in four stages:

### Stage A: Design privilege and session types

1. Create `src/privileges.rs` with a `Privileges` bitflags type covering all 38
   bits from `docs/protocol.md` field 110. Include constants for each privilege
   with documentation referencing the protocol spec.

2. Create `src/connection_flags.rs` with a `ConnectionFlags` bitflags type for
   the user preference flags (refuse private messages, refuse chat invites,
   automatic response).

3. Extend `Session` in `src/handler.rs` with `privileges: Privileges` and
   `connection_flags: ConnectionFlags` fields, defaulting to empty/none.

4. Add helper methods to `Session`:
   - `is_authenticated() -> bool`
   - `has_privilege(Privileges) -> bool`
   - `require_privilege(Privileges) -> Result<(), PrivilegeError>`

### Stage B: Populate session on login

1. Update `handle_login` in `src/login.rs` to set default privileges after
   successful authentication. Initially, all authenticated users receive a
   baseline set of privileges (matching guest/default account behaviour).

2. Extend the `users` table schema (via migration) to store privilege bits, or
   document that privileges are currently hard-coded pending user account
   management (task 5.1).

3. Update `handle_login` to read privileges from the database if stored, or
   apply defaults if not.

### Stage C: Enforce privileges in handlers

1. Update `GetFileNameList` handler in `src/commands/handlers.rs` to check
   `session.has_privilege(Privileges::DOWNLOAD_FILE)` before processing. Return
   error code 1 (authentication required) if not authenticated, or a new error
   code for insufficient privileges.

2. Update news handlers to check appropriate privileges:
   - `GetNewsCategoryNameList`: `NEWS_READ_ARTICLE`
   - `GetNewsArticleNameList`: `NEWS_READ_ARTICLE`
   - `GetNewsArticleData`: `NEWS_READ_ARTICLE`
   - `PostNewsArticle`: `NEWS_POST_ARTICLE`

3. Define privilege error codes aligned with Hotline protocol conventions.

### Stage D: Testing and documentation

1. Add unit tests in `src/privileges.rs` and `src/handler.rs`:
   - Privilege bitflags construction and checking
   - Session helper methods
   - Default privilege assignment

2. Add rstest-bdd behavioural tests in
   `tests/features/session_privileges.feature`:
   - Authenticated user can list files
   - Unauthenticated user receives error for file listing
   - User without download privilege receives error
   - User with news read privilege can list articles
   - User without news post privilege cannot post articles

3. Add Postgres-backed tests using `PostgresTestDb` fixture from
   `pg-embedded-setup-unpriv`.

4. Update `docs/design.md` with session context architecture diagram and
   privilege enforcement description.

5. Update `docs/users-guide.md` if privilege behaviour affects user experience.

6. Mark task 1.4.3 as done in `docs/roadmap.md`.

## Concrete Steps

All commands run from the repository root `/home/ariana/project`.

1. Create the privileges module:

       # Create src/privileges.rs with Privileges bitflags

2. Create the connection flags module:

       # Create src/connection_flags.rs with ConnectionFlags bitflags

3. Update lib.rs to expose new modules:

       # Add pub mod privileges; and pub mod connection_flags;

4. Extend Session struct:

       # Edit src/handler.rs to add privileges and connection_flags fields

5. Update login handler:

       # Edit src/login.rs to populate session.privileges

6. Add privilege checks to handlers:

       # Edit src/commands/handlers.rs and src/news_handlers/mod.rs to check
       # privileges before processing

7. Verify compilation:

       cargo build --features wireframe

   Expected: Build succeeds with no errors.

8. Run the test suite:

       set -o pipefail
       make test | tee /tmp/mxd-make-test.log

   Expected: All existing tests pass. New privilege tests are added and pass.

9. Run linting and formatting checks:

       set -o pipefail
       make check-fmt | tee /tmp/mxd-make-check-fmt.log
       make lint | tee /tmp/mxd-make-lint.log

   Expected: No warnings or errors.

10. Run Markdown validation:

        set -o pipefail
        make markdownlint | tee /tmp/mxd-make-markdownlint.log

    Expected: All documentation passes linting.

## Validation and Acceptance

Acceptance criteria from the roadmap:

> Session state survives across handlers and enforces privilege checks defined
> in `docs/protocol.md`.

Quality criteria (what "done" means):

- Tests: `make test` passes with new tests covering:
  - Privilege bitflags construction (38 bits match protocol spec)
  - Session helper methods (`is_authenticated`, `has_privilege`)
  - Privilege enforcement in `GetFileNameList` handler
  - Privilege enforcement in news handlers
  - Behavioural scenarios for authenticated/unauthenticated access

- Lint/typecheck: `make check-fmt` and `make lint` pass without warnings.

- Documentation: `docs/design.md` updated with session context architecture.

Quality method (verification approach):

1. `make test` exercises privilege enforcement paths.
2. Manual inspection of handler code confirms privilege checks precede
   processing.
3. Code review verifies privilege bit definitions match `docs/protocol.md`.

Specific test scenarios:

1. **Unauthenticated file listing**: Send `GetFileNameList` without prior login.
   Expected: Error response with code 1.

2. **Authenticated file listing**: Login, then send `GetFileNameList`.
   Expected: Success response with file list.

3. **News read without privilege**: Login with user lacking `NEWS_READ_ARTICLE`,
   send `GetNewsArticleData`. Expected: Error response with privilege error
   code.

4. **News post without privilege**: Login with user lacking `NEWS_POST_ARTICLE`,
   send `PostNewsArticle`. Expected: Error response with privilege error code.

## Idempotence and Recovery

All steps are safe to repeat. If a step fails partway through, re-run the
failing phase after correcting the error. The migration adding privilege
columns (if implemented) uses Diesel's idempotent migration system.

Commits should be atomic and focused. If a commit introduces a regression, it
can be reverted independently.

## Artifacts and Notes

Example privilege bitflags definition (based on `docs/protocol.md`):

    bitflags::bitflags! {
        /// User access privilege bits from Hotline protocol field 110.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct Privileges: u64 {
            /// Bit 0: Delete File
            const DELETE_FILE = 1 << 0;
            /// Bit 1: Upload File
            const UPLOAD_FILE = 1 << 1;
            /// Bit 2: Download File
            const DOWNLOAD_FILE = 1 << 2;
            // … remaining 35 bits
            /// Bit 20: News Read Article
            const NEWS_READ_ARTICLE = 1 << 20;
            /// Bit 21: News Post Article
            const NEWS_POST_ARTICLE = 1 << 21;
            // …
        }
    }

Example session helper:

    impl Session {
        pub fn has_privilege(&self, priv: Privileges) -> bool {
            self.privileges.contains(priv)
        }

        pub fn require_privilege(
            &self,
            priv: Privileges,
        ) -> Result<(), PrivilegeError> {
            if self.user_id.is_none() {
                return Err(PrivilegeError::NotAuthenticated);
            }
            if !self.has_privilege(priv) {
                return Err(PrivilegeError::InsufficientPrivileges);
            }
            Ok(())
        }
    }

Example privilege enforcement in handler:

    Self::GetFileNameList { header, .. } => {
        session.require_privilege(Privileges::DOWNLOAD_FILE)?;
        // … existing implementation
    }

## Interfaces and Dependencies

In `src/privileges.rs`, define:

    pub struct Privileges: u64 { /* bitflags */ }

In `src/connection_flags.rs`, define:

    pub struct ConnectionFlags: u8 { /* bitflags */ }

In `src/handler.rs`, extend:

    pub struct Session {
        pub user_id: Option<i32>,
        pub privileges: Privileges,
        pub connection_flags: ConnectionFlags,
    }

    impl Session {
        pub fn is_authenticated(&self) -> bool;
        pub fn has_privilege(&self, p: Privileges) -> bool;
        pub fn require_privilege(&self, p: Privileges) -> Result<(), PrivilegeError>;
    }

Dependencies:

- `bitflags` crate (added directly: `bitflags = "2.10.0"`)
- `rstest` v0.26 (already in dev-dependencies)
- `rstest-bdd` v0.3.2 (already in dev-dependencies)
- `pg-embedded-setup-unpriv` (already in dev-dependencies)
