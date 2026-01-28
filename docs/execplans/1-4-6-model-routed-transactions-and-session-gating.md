# Task 1.4.6: Model routed transactions and session gating in stateright

This Execution Plan (ExecPlan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

This document must be maintained in accordance with the execplans skill
instructions.

## Purpose / big picture

After this change, the mxd codebase will have formal verification guaranteeing
that privileged operations cannot occur before authentication. A Stateright
model will explore all possible interleavings of login, privilege checks, and
out-of-order message delivery across multiple concurrent client sessions.

Observable success: Running `cargo test -p mxd-verification` passes, including
new tests that exercise the Stateright session model. The model checker reports
no invariant violations and explores a non-trivial state space (> 100 unique
states).

## Constraints

- The `mxd-verification` crate must remain dependency-light. It must not depend
  on the main `mxd` crate (which would pull in async runtime and database
  types).
- Privilege bit constants in the verification model must mirror those in
  `src/privileges.rs` exactly. Any drift indicates a synchronization failure.
- The model must focus on the privilege enforcement semantics, not transport
  details (framing, codecs, sockets).
- No single code file may exceed 400 lines per AGENTS.md.
- All code must pass `make check-fmt`, `make lint`, and `make test`.
- Behaviour-Driven Development (BDD) tests must use `rstest-bdd` v0.3.2 as
  specified in the task.
- Documentation must use en-GB-oxendict spelling.

## Tolerances (exception triggers)

- **Scope**: If implementation requires changes to more than 10 files or 500
  lines of code (net), stop and escalate.
- **Interface**: If any public application programming interface (API) in the
  main `mxd` crate must change, stop and escalate.
- **Dependencies**: If a dependency other than `stateright` or `rstest-bdd` is
  required in `mxd-verification`, stop and escalate.
- **Iterations**: If tests still fail after 3 debug/fix cycles, stop and
  escalate.
- **Ambiguity**: If multiple valid interpretations exist for any invariant,
  stop and present options.

## Risks

- Risk: Stateright state space explosion with too many clients or queue depth.
  Severity: medium. Likelihood: medium. Mitigation: Start with conservative
  bounds (2 clients, queue depth 2), verify model completes in reasonable time,
  then expand bounds in separate test cases.

- Risk: Privilege constant drift between `src/privileges.rs` and model.
  Severity: low. Likelihood: low. Mitigation: Add explicit comments noting the
  source of truth; consider a compile-time or continuous integration (CI) check
  in future.

- Risk: Out-of-order delivery model may not capture all real-world reordering
  scenarios. Severity: low. Likelihood: medium. Mitigation: Queue-based model
  with non-deterministic index selection covers essential reordering; more
  sophisticated network models can be added later if needed.

## Progress

- [x] Stage A: Add Stateright dependency to mxd-verification Cargo.toml.
- [x] Stage B: Create privilege constants module mirroring src/privileges.rs.
- [x] Stage C: Create state types (ModelSession, SystemState, Effect).
- [x] Stage D: Create action types and transition logic.
- [x] Stage E: Create invariant property definitions.
- [x] Stage F: Implement Stateright Model trait for SessionModel.
- [x] Stage G: Create test harness with verification tests.
- [x] Stage H: Add BDD tests for session gating verification scenarios.
- [x] Stage I: Update documentation (verification-strategy.md, users-guide.md).
- [x] Stage J: Run quality gates (fmt, lint, test, markdownlint).
- [x] Stage K: Mark roadmap entry as complete.

## Surprises & discoveries

(To be populated during implementation.)

## Decision log

- Decision: Create a pure model in `mxd-verification` rather than importing
  `Session`/`Privileges` from the main crate. Rationale: Keeps verification
  crate dependency-light and avoids pulling in async runtime and database
  types. Importing would require feature gates and complicate the build.
  Date/Author: Plan phase.

- Decision: Use a queue-based out-of-order delivery model rather than a full
  network actor model. Rationale: Simpler state space, captures essential
  reordering semantics without network topology complexity. A queue per client
  with non-deterministic delivery index selection models the key scenario
  (requests arriving in different order than sent). Date/Author: Plan phase.

- Decision: Track effects history for temporal invariants. Rationale: The core
  invariant "authentication precedes privileged effect" requires temporal
  reasoning about event ordering. Maintaining the effects log in the state
  enables expressing this as a state invariant. Date/Author: Plan phase.

## Outcomes & retrospective

(To be populated at completion.)

## Context and orientation

### Repository structure

The mxd codebase is a Hotline protocol server implementation in Rust. Key
directories:

- `src/` — Main crate with server implementation, for example:
  - `src/handler.rs` — Session struct with `user_id`, `privileges`,
    `connection_flags`, and privilege checking methods
  - `src/privileges.rs` — 38-bit privilege flags matching Hotline protocol
    field 110
  - `src/commands/` — Transaction handlers with privilege enforcement
- `crates/mxd-verification/` — Verification crate containing:
  - `tla/MxdHandshake.tla` — Temporal Logic of Actions (TLA+) handshake
    state machine
  - `tests/tlc_handshake.rs` — TLA+ model checker (TLC) integration test
- `docs/` — Documentation, for example:
  - `docs/verification-strategy.md` — Three-tier verification approach
  - `docs/roadmap.md` — Project roadmap with task 1.4.6

### Session and privilege system

The Session struct tracks per-connection authentication state:

    pub struct Session {
        pub user_id: Option<i32>,           // None = unauthenticated
        pub privileges: Privileges,          // 38-bit bitmap
        pub connection_flags: ConnectionFlags,
    }

Key methods:

- `is_authenticated()` — Returns true if `user_id` is `Some`
- `has_privilege(priv_bit)` — Returns true if authenticated AND has the bit set
- `require_privilege(priv_bit)` — Returns `Ok(())` or `PrivilegeError`
- `require_authenticated()` — Returns `Ok(())` or
  `PrivilegeError::NotAuthenticated`

Error codes: 1 = not authenticated, 4 = insufficient privileges.

### Verification strategy

The project uses a three-tier verification approach:

1. TLA+ and TLC for abstract state machines (handshake spec exists)
2. Stateright for executable models with concurrency (this task)
3. Kani for local invariants (codec harnesses exist)

Stateright models should focus on concurrency, client interleavings, and
ordering properties. They live in `crates/mxd-verification/`.

### Acceptance criteria

From `docs/roadmap.md` lines 155-159:

- Stateright models explore login, privilege checks, and out-of-order delivery
- `cargo test -p mxd-verification` passes
- Invariants prevent privileged effects before authentication

## Plan of work

### Stage A: Add Stateright dependency

Update `crates/mxd-verification/Cargo.toml` to add the `stateright` crate as a
dependency. The crate currently uses edition 2024 and has minimal dependencies.

### Stage B: Create privilege constants module

Create `crates/mxd-verification/src/session_model/privileges.rs` with constants
mirroring the bit positions from `src/privileges.rs`. Include only the
privileges needed for verification (DOWNLOAD_FILE, NEWS_READ_ARTICLE,
NEWS_POST_ARTICLE) plus a composite constant for DEFAULT_USER_PRIVILEGES.

### Stage C: Create state types

Create `crates/mxd-verification/src/session_model/state.rs` with:

- `ModelSession` — Tracks `user_id: Option<u32>` and `privileges: u64`
- `RequestType` — Enum of request types with required privilege mapping
- `ModelMessage` — Wrapper for queued requests
- `Effect` — Enum tracking observable outcomes (authenticated, rejected,
  privileged effect completed)
- `SystemState` — Global state with per-client sessions, queues, and effects

### Stage D: Create action types and transitions

Create `crates/mxd-verification/src/session_model/actions/mod.rs` with:

- `Action` enum — Login, Logout, SendRequest, DeliverRequest
- `apply_action(state, action) -> state'` — Pure transition function that
  implements privilege checking logic matching `src/handler.rs` semantics

### Stage E: Create invariant property definitions

Create `crates/mxd-verification/src/session_model/properties.rs` with:

- `no_privileged_effect_without_auth()` — Safety property
- `no_privileged_effect_without_required_privilege()` — Safety property
- `authentication_precedes_privileged_effect()` — Temporal safety property
- `can_reject_unauthenticated()` — Reachability property (sometimes)
- `can_complete_privileged_operation()` — Reachability property (sometimes)

### Stage F: Implement Stateright Model trait

Create `crates/mxd-verification/src/session_model/mod.rs` with:

- `SessionModel` struct with configuration (num_clients, max_queue_depth, etc.)
- `Model` trait implementation with init_states, actions, next_state, properties
- Default configuration targeting 2 clients, queue depth 2, 4 privilege sets.

Update `crates/mxd-verification/src/lib.rs` to export the new module.

### Stage G: Create test harness

Create `crates/mxd-verification/tests/session_gating.rs` with rstest tests:

- `session_model_verifies_with_default_config` — Main verification test
- `session_model_explores_nontrivial_state_space` — Confirms adequate coverage
- `session_model_verifies_single_client` — Minimal configuration
- `session_model_verifies_concurrent_clients` — Stress test with 3 clients

### Stage H: Add BDD tests

Create `tests/features/session_gating_verification.feature` with scenarios:

- Stateright model verifies no privileged effects without authentication
- Stateright model explores out-of-order delivery
- Stateright model completes within reasonable bounds

Create `crates/mxd-verification/tests/session_gating_bdd.rs` with step
definitions using rstest-bdd v0.3.2.

### Stage I: Update documentation

Update `docs/verification-strategy.md` to document the new SessionModel,
including its purpose, configuration bounds, invariants, and run commands.

Update `docs/design.md` if session gating architecture section needs revision.

Review `docs/users-guide.md` for any user-visible changes (likely none for
internal verification).

### Stage J: Run quality gates

Execute:

- `make check-fmt` — Verify formatting
- `make lint` — Run Clippy
- `make test` — Run full test suite including new verification tests
- `make markdownlint` — Validate documentation

### Stage K: Mark roadmap complete

Update `docs/roadmap.md` to mark task 1.4.6 as complete with status note.

## Concrete steps

All commands run from the repository root.

### 1. Add Stateright dependency

Edit `crates/mxd-verification/Cargo.toml`:

    [dependencies]
    stateright = "0.30"

Verify dependency resolves:

    cargo check -p mxd-verification

Expected: Compilation succeeds with no errors.

### 2. Create module structure

Create directory and files:

    crates/mxd-verification/src/session_model/mod.rs
    crates/mxd-verification/src/session_model/privileges.rs
    crates/mxd-verification/src/session_model/state.rs
    crates/mxd-verification/src/session_model/actions/mod.rs
    crates/mxd-verification/src/session_model/properties.rs

### 3. Run verification tests

    cargo test -p mxd-verification -- session_gating

Expected output (sample):

    running 4 tests
    test session_model_verifies_with_default_config … ok
    test session_model_explores_nontrivial_state_space … ok
    test session_model_verifies_single_client … ok
    test session_model_verifies_concurrent_clients … ok

    test result: ok. 4 passed; 0 failed

### 4. Run quality gates

    make check-fmt && make lint && make test && make markdownlint

Expected: All commands succeed with exit code 0.

## Validation and acceptance

Quality criteria (what "done" means):

- Tests: `cargo test -p mxd-verification` passes, including all session_gating
  tests
- Lint/typecheck: `make check-fmt` and `make lint` pass with no warnings
- Verification: Stateright model checker reports no property violations
- Coverage: Model explores > 100 unique states (confirms non-trivial coverage)
- Documentation: `make markdownlint` passes

Quality method (how to check):

    # Run the full quality gate suite
    make check-fmt && make lint && make test && make markdownlint

    # Specifically verify the Stateright model
    cargo test -p mxd-verification -- session_gating --nocapture

The test output should show:

1. No failed assertions
2. State count > 100 (logged by the state space test)
3. All properties (safety and reachability) satisfied

## Idempotence and recovery

All steps are idempotent:

- File creation/editing can be repeated safely
- Tests can be re-run without side effects
- No database state or external resources are modified

If a step fails partway through:

- Fix the issue and re-run the failing command
- Previously completed steps do not need to be repeated

Rollback: If the entire change needs to be reverted, `git checkout` the
affected files or reset to the previous commit.

## Artifacts and notes

### Key privilege bit constants (from src/privileges.rs)

    DOWNLOAD_FILE        = 1 << 2   // File listing
    READ_CHAT            = 1 << 9   // Chat read
    SEND_CHAT            = 1 << 10  // Chat send
    SHOW_IN_LIST         = 1 << 13  // User list visibility
    SEND_PRIVATE_MESSAGE = 1 << 19  // Private messages
    NEWS_READ_ARTICLE    = 1 << 20  // News read
    NEWS_POST_ARTICLE    = 1 << 21  // News post
    GET_CLIENT_INFO      = 1 << 24  // Client info
    CHANGE_OWN_PASSWORD  = 1 << 18  // Password change

### Default user privileges composite

    DEFAULT_USER_PRIVILEGES = DOWNLOAD_FILE | READ_CHAT | SEND_CHAT |
                              SHOW_IN_LIST | SEND_PRIVATE_MESSAGE |
                              NEWS_READ_ARTICLE | NEWS_POST_ARTICLE |
                              GET_CLIENT_INFO | CHANGE_OWN_PASSWORD

### Session privilege error codes

    ERR_NOT_AUTHENTICATED = 1
    ERR_INSUFFICIENT_PRIVILEGES = 4

## Interfaces and dependencies

### External dependency

    stateright = "0.30"
    rstest-bdd = "0.3.2"

Stateright is a model-checking framework for Rust that explores all reachable
states of a system model to verify safety and liveness properties.

### Module interface (crates/mxd-verification/src/session_model/mod.rs)

    pub struct SessionModel {
        pub num_clients: usize,
        pub max_queue_depth: usize,
        pub user_ids: Vec<u32>,
        pub privilege_sets: Vec<u64>,
    }

    impl stateright::Model for SessionModel {
        type State = SystemState;
        type Action = Action;

        fn init_states(&self) -> Vec<Self::State>;
        fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>);
        fn next_state(&self, state: &Self::State, action: Self::Action)
            -> Option<Self::State>;
        fn properties(&self) -> Vec<Property<Self>>;
    }

### State types (crates/mxd-verification/src/session_model/state.rs)

    pub struct ModelSession {
        pub user_id: Option<u32>,
        pub privileges: u64,
    }

    pub enum RequestType {
        Ping,
        GetUserInfo,
        GetClientInfo,
        GetFileList,
        GetNewsCategories,
        PostNewsArticle,
    }

    pub enum Effect {
        Authenticated { client: usize, user_id: u32 },
        RejectedUnauthenticated { client: usize, request: RequestType },
        RejectedInsufficient { client: usize, request: RequestType, required: u64 },
        PrivilegedEffect { client: usize, request: RequestType, privilege: u64 },
        UnprivilegedEffect { client: usize, request: RequestType },
    }

    pub struct SystemState {
        pub sessions: Vec<ModelSession>,
        pub queues: Vec<Vec<ModelMessage>>,
        pub effects: Vec<Effect>,
    }

### Action types (crates/mxd-verification/src/session_model/actions/mod.rs)

    pub enum Action {
        Login { client: usize, user_id: u32, privileges: u64 },
        Logout { client: usize },
        SendRequest { client: usize, request: RequestType },
        DeliverRequest { client: usize, queue_index: usize },
    }

    pub fn apply_action(state: &SystemState, action: &Action) -> SystemState;

### Properties (crates/mxd-verification/src/session_model/properties.rs)

    pub fn no_privileged_effect_without_auth() -> Property<SessionModel>;
    pub fn no_privileged_effect_without_required_privilege() -> Property<SessionModel>;
    pub fn authentication_precedes_privileged_effect() -> Property<SessionModel>;
    pub fn can_reject_unauthenticated() -> Property<SessionModel>;
    pub fn can_complete_privileged_operation() -> Property<SessionModel>;
