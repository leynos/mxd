# Implementation roadmap

This roadmap sequences the work required to deliver a Hotline-compatible server
on top of the `wireframe` transport. It consolidates requirements captured in
<!-- markdownlint-disable-next-line MD013 -->
`docs/design.md`,
`docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`,
`docs/protocol.md`, `docs/file-sharing-design.md`,
`docs/cte-extension-design.md`, `docs/chat-schema.md`, `docs/news-schema.md`,
`docs/fuzzing.md`, `docs/verification-strategy.md`, and
`docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md`. Items
are organised into phases, steps, and measurable tasks with acceptance criteria
and explicit dependencies. Timeframes are intentionally omitted.

## 1. Wireframe migration

### 1.1. Bootstrap the wireframe server

- [x] 1.1.1. Extract protocol and domain logic into reusable library modules.
  Acceptance: `mxd` builds as a library crate consumed by both binaries,
  existing integration smoke tests pass unchanged, and core modules remain free
  of `wireframe::*` imports as prescribed in `docs/design.md`. Status:
  Completed on 9 November 2025 by moving the CLI and Tokio runtime into the
  shared `mxd::server` module, so every binary reuses the same entry points.
  Dependencies: None.
- [x] 1.1.2. Create the `mxd-wireframe-server` binary that depends on
  `wireframe` and the refactored library. Acceptance: The new binary compiles
  for x86_64 and aarch64 Linux and exposes a minimal listen loop that loads
  configuration. Status: Completed on 18 November 2025 by adding
  `WireframeBootstrap` (`src/server/wireframe.rs`) and a new binary entry point
  so the listener binds after parsing `AppConfig`. Dependencies: 1.1.1.
- [x] 1.1.3. Retire the bespoke networking loop once the wireframe pipeline is
  feature-complete. Acceptance: The legacy frame handler is gated behind a
  feature flag or removed without reducing existing automated test coverage,
  aligning with the adapter strategy in
  `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`.
  Status: Completed on 20 November 2025 by gating the legacy runtime behind the
  `legacy-networking` feature, defaulting `server::run()` to Wireframe when
  that feature is disabled, and preserving test coverage via added unit,
  behaviour, and Postgres-backed admin tests. Dependencies: 1.4.

### 1.2. Implement the wireframe handshake

- [x] 1.2.1. Implement the 12-byte Hotline handshake preamble as a
  `wireframe::preamble::Preamble`. Acceptance: Unit tests accept the "TRTP"
  protocol ID and reject malformed inputs outlined in `docs/protocol.md`.
  Status: Completed on 24 November 2025 by introducing `HotlinePreamble` as the
  Wireframe decoder and adding unit and behaviour tests for valid and invalid
  greetings. Dependencies: 1.1.
- [x] 1.2.2. Register success and failure hooks that emit the 8-byte reply and
  enforce a five-second timeout. Acceptance: Handshake errors surface correct
  Hotline error codes and time out idle sockets, matching behaviour documented
  in the migration plan. Status: Completed on 29 November 2025 by switching to
  `wireframe` v0.1.0's preamble hooks, sending Hotline reply codes for success
  and validation errors, enforcing the five-second timeout, and covering the
  paths with `rstest` and `rstest-bdd` suites. Dependencies: 1.2.1.
- [x] 1.2.3. Persist handshake metadata (sub-protocol ID, sub-version) into
  per-connection state for later routing decisions. Acceptance: Subsequent
  handlers can branch on the stored metadata to decide compatibility shims.
  Status: Completed on 2 December 2025 by recording handshake metadata per
  connection task, exposing it via connection state and app data, and clearing
  it on teardown, so routing can gate compatibility shims safely. Dependencies:
  1.2.1.
- [ ] 1.2.4. Model handshake readiness in Temporal Logic of Actions (TLA+) and
  the TLC (TLA+ model checker). Acceptance: TLC runs the
  `crates/mxd-verification/tla/MxdHandshake.tla` spec with no invariant
  violations for bounded client counts and documents timeout, error-code, and
  readiness invariants. Dependencies: Task “Persist handshake metadata
  (sub-protocol ID, sub-version) into per-connection state for later routing
  decisions.”

### 1.3. Adopt wireframe transaction framing

- [x] 1.3.1. Build a `wireframe` codec that reads and writes the 20-byte
  transaction header and payload framing described in `docs/protocol.md`.
  Acceptance: Property tests cover multi-fragment requests and reject invalid
  length combinations. Status: Completed on 9 December 2025 by implementing
  `BorrowDecode` for `HotlineTransaction` with header validation,
  multi-fragment reassembly, and comprehensive property tests. Dependencies:
  1.2.
- [x] 1.3.2. Surface a streaming API for large payloads so file transfers and
  news posts can consume fragmented messages incrementally. Acceptance:
  Integration tests stream upload and download payloads across multiple
  fragments, verify that reconstructed payloads exactly match the originals,
  and keep peak memory usage below the configured streaming limit (no buffer
  exhaustion). Status: Completed on 12 December 2025 by adding
  `TransactionStreamReader`, `StreamingTransaction`, and
  `TransactionWriter::write_streaming` with configurable total-size limits and
  BDD coverage for multi-fragment streaming. Dependencies: 1.3.1.
- [x] 1.3.3. Reuse existing parameter encoding helpers within the new codec to
  prevent duplicate implementations. Acceptance: All transactions built through
  the new codec match the byte-for-byte output of the existing encoder for
  shared cases. Status: Completed on 19 December 2025 by adding outbound
  encoding support to `HotlineTransaction`, reusing
  `transaction::encode_params` for parameter payloads, and covering parity with
  the legacy writer via `rstest-bdd` scenarios. Dependencies: 1.3.1.
- [ ] 1.3.4. Add Kani harnesses for transaction framing invariants. Acceptance:
  Kani proves header validation, fragment sizing, and transaction ID echoing
  for bounded payloads without panics. Dependencies: Task “Reuse existing
  parameter encoding helpers within the new codec to prevent duplicate
  implementations.”

### 1.4. Route transactions through wireframe

- [x] 1.4.1. Implement a domain-backed `WireframeProtocol` adapter registered
  via `.with_protocol(...)`. Acceptance: The server initialization builds the
  adapter described in
  `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`, and
  routing smoke tests exercise every handler through this port without invoking
  legacy wiring. Status: Completed on 25 December 2025 by introducing
  `HotlineProtocol` implementing `WireframeProtocol` with lifecycle hooks,
  `RouteState` and `SessionState` for per-connection context, and unit tests
  covering protocol adapter registration, error routing, and transaction ID
  preservation. Dependencies: 1.3.
- [x] 1.4.2. Map every implemented transaction ID to a `wireframe` route that
  delegates to the existing domain handlers. Acceptance: Login, news listing,
  and file listing integration tests run against the wireframe server without
  code duplication. Status: Completed on 4 January 2026 by routing transactions
  through `mxd-wireframe-server` with the `HotlineFrameCodec`, adding routing
  unit/BDD coverage, and migrating the file/news integration tests.
  Dependencies: 1.4.1.
- [ ] 1.4.3. Introduce a shared session context that tracks the authenticated
  user, privileges, and connection flags. Acceptance: Session state survives
  across handlers and enforces privilege checks defined in `docs/protocol.md`.
  Dependencies: 1.4.2.
- [ ] 1.4.4. Provide outbound transport and messaging traits so domain code can
  respond without depending on `wireframe` types. Acceptance: Domain modules
  interact with adapter traits defined alongside the server boundary, and the
  crate continues compiling with no direct `wireframe` imports, matching
  guidance in `docs/design.md`. Dependencies: 1.4.2.
- [ ] 1.4.5. Provide a reply builder that mirrors Hotline error propagation and
  logging conventions. Acceptance: Error replies retain the original
  transaction IDs and are logged through the existing tracing infrastructure.
  Dependencies: 1.4.4.
- [ ] 1.4.6. Model routed transactions and session gating in Stateright.
  Acceptance: Stateright models explore login, privilege checks, and
  out-of-order delivery, and `cargo test -p mxd-verification` passes with
  invariants preventing privileged effects before authentication. Dependencies:
  Task “Provide a reply builder that mirrors Hotline error propagation and
  logging conventions.”

### 1.5. Validate Hotline and SynHX compatibility

- [ ] 1.5.1. Detect clients that XOR-encode text fields and transparently decode
  or encode responses when required. Acceptance: SynHX parity tests cover
  password, message, and news bodies with the XOR toggle enabled. Dependencies:
  1.2.
- [ ] 1.5.2. Gate protocol quirks on the handshake sub-version so Hotline 1.9
  fallbacks remain available. Acceptance: Compatibility tests prove Hotline
  1.8.5, Hotline 1.9, and SynHX all log in, list users, and exchange messages
  successfully. Dependencies: 1.2.3.
- [ ] 1.5.3. Publish an internal compatibility matrix documenting supported
  clients, known deviations, and required toggles. Acceptance: The matrix lives
  in `docs/` and is referenced by release notes during QA sign-off.
  Dependencies: 1.5.2.
- [ ] 1.5.4 Verify XOR and sub-version compatibility logic with Kani.
  Acceptance: Kani harnesses show XOR encode/decode round-trips and version
  gating for bounded inputs without panics. Dependencies: Task “Gate protocol
  quirks on the handshake sub-version so Hotline 1.9 fallbacks remain
  available.”

### 1.6. Regression and platform verification

- [ ] 1.6.1. Port unit and integration tests so they start the wireframe server
  binary under test. Acceptance: `cargo test` exercises login, presence, file
  listing, and news flows against the new binary. Dependencies: 1.4.
- [ ] 1.6.2. Extend the hx-based validator harness to target the wireframe
  server. Acceptance: The harness covers login, file download, chat, and news
  flows and runs in CI. Dependencies: 1.6.1.
- [ ] 1.6.3. Add cross-architecture CI jobs (x86_64 and aarch64 Linux) for the
  wireframe build and smoke tests. Acceptance: CI publishes binaries for both
  targets and reports handshake, login, and shutdown smoke results.
  Dependencies: 1.6.1.
- [ ] 1.6.4. Add CI checks for formal verification artefacts. Acceptance: CI
  runs Stateright models, selected Kani harnesses, and TLC specs, publishing
  counterexample traces as build artefacts. Dependencies: Task “Port unit and
  integration tests so they start the wireframe server binary under test.”

## 2. Session and presence parity

### 2.1. Harden session lifecycle management

- [ ] 2.1.1. Implement transactions 300–307 (user list, change notifications,
  agree/disagree flows) exactly as described in `docs/protocol.md`. Acceptance:
  Clients receive Notify Change User (301) and Notify Delete User (302)
  broadcasts that reflect session joins, updates, and logouts. Dependencies: 1.
- [ ] 2.1.2. Track presence state, idle timers, and away flags in the shared
  session context. Acceptance: Auto-away and idle timeout thresholds trigger
  updates in the user list without manual refresh. Dependencies: 2.1.1.
- [ ] 2.1.3. Expose admin tooling to inspect and terminate sessions by user ID
  or connection ID. Acceptance: Administrators can enumerate sessions and
  terminate connections using the CLI, with events mirrored to all clients.
  Dependencies: 2.1.2.
- [ ] 2.1.4. Model session lifecycle and presence interleavings in Stateright.
  Acceptance: Stateright models cover login, agree/disagree, idle updates, and
  logout ordering across multiple clients with invariants matching
  `docs/protocol.md`. Dependencies: Task “Track presence state, idle timers,
  and away flags in the shared session context.”

### 2.2. Implement private messaging workflows

- [ ] 2.2.1. Support Send Instant Message (108) and Server Message (104)
  transactions including quoting and automatic responses. Acceptance: Unit
  tests validate option codes 1–4 and quoted replies defined in
  `docs/protocol.md`. Dependencies: 2.1.
- [ ] 2.2.2. Enforce privilege code 19 (Send Private Message) and refusal flags
  surfaced by Set Client User Info (304). Acceptance: Users without privilege
  receive error replies and refusal flags block delivery in integration tests.
  Dependencies: 2.2.1.
- [ ] 2.2.3. Log private message metadata (sender, recipient, timestamp, option)
  for auditing without storing message bodies. Acceptance: Audit records
  support tracing abuse reports while respecting privacy requirements.
  Dependencies: 2.2.2.
- [ ] 2.2.4. Specify private messaging delivery rules in TLA+ and check with
  TLC. Acceptance: TLC verifies refusal flags and privilege gating for bounded
  sender and recipient sets with no invariant violations. Dependencies: Task
  “Enforce privilege code 19 (Send Private Message) and refusal flags surfaced
  by Set Client User Info (304).”

### 2.3. Deliver chat room operations

- [ ] 2.3.1. Apply the schema in `docs/chat-schema.md` via Diesel migrations and
  model structs. Acceptance: Tables `chat_rooms`, `chat_participants`,
  `chat_messages`, and `chat_invites` exist in SQLite and PostgreSQL
  migrations. Dependencies: 1.
- [ ] 2.3.2. Implement chat transactions (Create Chat 111, Invite 112–114, Join
  115, Leave 116, Send Chat 105, Chat Message 106, Notify Chat events 117–120)
  exactly as specified in `docs/protocol.md`. Acceptance: Multi-user
  integration tests verify room creation, invitations, subjects, and broadcast
  messages. Dependencies: 2.3.1.
- [ ] 2.3.3. Persist room transcripts and enforce retention policies (per-room
  limits or duration-based pruning). Acceptance: Transcript retrieval returns
  chronological chat history within configured limits and purges older rows
  automatically. Dependencies: 2.3.2.
- [ ] 2.3.4. Model chat room membership and invite flows in Stateright.
  Acceptance: Stateright explores create, invite, join, leave, and message
  ordering, proving membership invariants and correct broadcast recipients.
  Dependencies: Task “Implement chat transactions (Create Chat 111, Invite
  112–114, Join 115, Leave 116, Send Chat 105, Chat Message 106, Notify Chat
  events 117–120) exactly as specified in `docs/protocol.md`.”

## 3. File services parity

### 3.1. Align file metadata and access control

- [ ] 3.1.1. Introduce the `FileNode` schema and permission model described in
  `docs/file-sharing-design.md`. Acceptance: Diesel migrations produce tables
  with folder/file alias support and integrate with the shared permission
  tables. Dependencies: 1.
- [ ] 3.1.2. Migrate existing file metadata into `FileNode` records without data
  loss. Acceptance: All legacy file entries appear in the new schema with
  correct hierarchy, comments, and ACLs. Dependencies: 3.1.1.
- [ ] 3.1.3. Implement caching or indexing to accelerate frequent directory
  listings. Acceptance: Directory list latency stays within agreed SLAs for
  repositories containing thousands of nodes. Dependencies: 3.1.2.
- [ ] 3.1.4. Add Kani harnesses for permission bitsets and drop box predicates.
  Acceptance: Kani proves access-control list (ACL) checks and drop box
  visibility rules for bounded cases without panics. Dependencies: Task
  “Introduce the `FileNode` schema and permission model described in
  `docs/file-sharing-design.md`.”

### 3.2. Provide file listing and metadata transactions

- [ ] 3.2.1. Implement Get File Name List (200) and Get File Info (206) using
  the new schema. Acceptance: Clients receive Hotline-compliant listing records
  including size, comments, and privilege flags. Dependencies: 3.1.
- [ ] 3.2.2. Implement Set File Info (207) for renames, comments, and drop box
  flags with privilege enforcement. Acceptance: Integration tests verify
  renames, comment edits, and drop box toggles with proper audit logs.
  Dependencies: 3.2.1.
- [ ] 3.2.3. Expose folder-specific ACL management commands to admins.
  Acceptance: Admins can grant or revoke folder privileges through CLI or
  administrative transactions with immediate effect. Dependencies: 3.2.2.
- [ ] 3.2.4. Model file listing visibility in Stateright. Acceptance:
  Stateright models include admin and non-admin clients and prove drop box
  contents never leak to unauthorised users, matching
  `docs/file-sharing-design.md`. Dependencies: Task “Implement Set File Info
  (207) for renames, comments, and drop box flags with privilege enforcement.”

### 3.3. Build transfer and resume pipelines

- [ ] 3.3.1. Implement Download File (202) with resume support and dedicated
  data-channel negotiation. Acceptance: Partial downloads resume correctly
  after reconnect and respect bandwidth throttling policies. Dependencies: 3.2.
- [ ] 3.3.2. Implement Upload File (203) and Upload Folder (213) with multipart
  streaming to `object_store` backends. Acceptance: Uploads can be paused and
  resumed and emit progress events to the client. Dependencies: 3.3.1.
- [ ] 3.3.3. Implement Download Folder (210) with recursive packaging and
  optional compression. Acceptance: Clients receive multi-file archives that
  reconstruct the folder hierarchy faithfully. Dependencies: 3.3.2.
- [ ] 3.3.4. Specify transfer and resume state machines in TLA+ and check with
  TLC. Acceptance: The spec models download and upload, resume tokens, and
  data-channel negotiation and proves idempotent completion for bounded
  transfers. Dependencies: Task “Implement Upload File (203) and Upload Folder
  (213) with multipart streaming to `object_store` backends.”

### 3.4. Deliver advanced file management features

- [ ] 3.4.1. Implement folder operations (New Folder 205, Move 208, Delete 204)
  with transactional integrity. Acceptance: Operations either commit fully or
  roll back on failure and honour folder-level privileges. Dependencies: 3.3.
- [ ] 3.4.2. Implement Make File Alias (209) with validation against cycles and
  permission inheritance. Acceptance: Alias creation respects access
  restrictions and exposes target metadata consistently. Dependencies: 3.4.1.
- [ ] 3.4.3. Surface drop box behaviours (upload-only folders, per-user mailbox)
  defined in `docs/file-sharing-design.md`. Acceptance: Drop boxes hide
  contents from non-admin users while accepting uploads and notifying
  moderators. Dependencies: 3.4.1.
- [ ] 3.4.4. Prove alias and move validation helpers with Kani. Acceptance:
  Kani harnesses prevent cycles, self-links, and invalid moves for bounded
  folder graphs. Dependencies: Task “Implement Make File Alias (209) with
  validation against cycles and permission inheritance.”

### 3.5. Support multi-backend object storage

- [ ] 3.5.1. Configure `object_store` drivers for local disk, S3-compatible, and
  Azure Blob targets. Acceptance: Integration tests upload and download files
  across all supported backends using the same code path. Dependencies: 3.3.
- [ ] 3.5.2. Implement lifecycle hooks for retention policies (expiry, archive,
  delete) per folder. Acceptance: Scheduled jobs archive or delete expired
  content and update listings accordingly. Dependencies: 3.5.1.
- [ ] 3.5.3. Instrument transfer metrics (latency, throughput, error rate) for
  observability dashboards. Acceptance: Metrics feed Grafana-style dashboards
  and alert on sustained regressions. Dependencies: 3.5.2.
- [ ] 3.5.4. Model object lifecycle transitions in TLA+ and check with TLC.
  Acceptance: The spec proves expiry, archive, and delete transitions are
  monotonic and do not resurrect content for bounded objects. Dependencies:
  Task “Implement lifecycle hooks for retention policies (expiry, archive,
  delete) per folder.”

## 4. News system rebuild

### 4.1. Align the news schema and migrations

- [ ] 4.1.1. Apply the schema from `docs/news-schema.md` (bundles, categories,
  threaded articles, permissions). Acceptance: Schema exists in both SQLite and
  PostgreSQL migrations with referential integrity and required indices.
  Dependencies: 1.
- [ ] 4.1.2. Migrate existing news content into the new structure with bundle
  and category GUIDs. Acceptance: Historical articles retain threading (parent,
  prev, next) and remain addressable by GUID. Dependencies: 4.1.1.
- [ ] 4.1.3. Seed the permissions catalogue with the 38 news privilege codes
  documented in `docs/protocol.md`. Acceptance: Users acquire news privileges
  via `user_permissions` entries and transactions honour those flags.
  Dependencies: 4.1.1.
- [ ] 4.1.4. Specify news threading invariants in TLA+ and check with TLC.
  Acceptance: The spec models parent, prev, next, and first-child links and
  proves acyclic ordering for bounded graphs. Dependencies: Task “Apply the
  schema from `docs/news-schema.md` (bundles, categories, threaded articles,
  permissions).”

### 4.2. Implement news browsing transactions

- [ ] 4.2.1. Implement Get News Category List, Get News Category, and Get News
  Article transactions with paging support. Acceptance: Clients can traverse
  bundles, categories, and threaded articles with consistent sequence numbers.
  Dependencies: 4.1.
- [ ] 4.2.2. Implement news search and filtering (by poster, date range,
  headline) using Diesel query helpers. Acceptance: Search queries return
  results within 200 ms for typical data sets and support CTE-backed recursive
  traversal where needed. Dependencies: 4.2.1.
- [ ] 4.2.3. Cache frequently accessed bundles and article headers.
  Acceptance: Cache hit rates exceed 90% for popular bundles without stale data
  exceeding configured TTLs. Dependencies: 4.2.2.
- [ ] 4.2.4. Add Kani harnesses for paging and sequence calculations.
  Acceptance: Kani proves bounds checks and monotonic sequence numbers for
  bounded result sets without panics. Dependencies: Task “Implement news search
  and filtering (by poster, date range, headline) using Diesel query helpers.”

### 4.3. Implement news authoring and moderation

- [ ] 4.3.1. Implement Post News, Post News Reply, Edit News, and Delete News
  transactions with full audit trails. Acceptance: Article revisions capture
  editor, timestamp, and diff metadata and enforce privilege codes 21 and 33.
  Dependencies: 4.1.
- [ ] 4.3.2. Implement category and bundle management transactions (create,
  rename, delete) with hierarchical updates. Acceptance: Bundle/category
  operations update GUIDs, sequence numbers, and notify subscribed clients.
  Dependencies: 4.3.1.
- [ ] 4.3.3. Provide moderation tooling for locking threads, pinning articles,
  and escalating reports. Acceptance: Moderators can lock or pin via the CLI or
  administrative transactions and clients reflect the state in listings.
  Dependencies: 4.3.1.
- [ ] 4.3.4. Model concurrent news edits and moderation in Stateright.
  Acceptance: Stateright explores post, reply, edit, delete, and lock ordering
  and proves linkage invariants and privilege gates. Dependencies: Task
  “Implement Post News, Post News Reply, Edit News, and Delete News
  transactions with full audit trails.”

## 5. Administration and database platform

### 5.1. Complete administrative protocol coverage

- [ ] 5.1.1. Implement administrative transactions (Kick User 109, Ban User,
  Broadcast 152, Server Message 104 without user ID) per `docs/protocol.md`.
  Acceptance: Administrators can manage sessions, issue broadcasts, and close
  the server gracefully. Dependencies: 2.
- [ ] 5.1.2. Enforce privilege codes 14–18 and 22–32 across administrative
  handlers. Acceptance: Privilege violations return protocol error codes and
  are logged with user context. Dependencies: 5.1.1.
- [ ] 5.1.3. Provide audit logs summarising administrative actions with before/
  after snapshots where applicable. Acceptance: Audit entries include actor,
  target, action, and rationale fields and feed compliance reporting.
  Dependencies: 5.1.2.
- [ ] 5.1.4. Model administrative actions and session termination in
  Stateright. Acceptance: Stateright proves kicks, bans, and broadcasts honour
  privilege gates and that termination is idempotent for bounded sessions.
  Dependencies: Task “Enforce privilege codes 14–18 and 22–32 across
  administrative handlers.”

### 5.2. Harden database backends and query tooling

- [ ] 5.2.1. Finalise PostgreSQL support, ensuring migrations, Diesel builders,
  and tests run against PostgreSQL 14+. Acceptance: CI runs the full
  integration suite on SQLite and PostgreSQL backends with identical behaviour.
  Dependencies: 3.
- [ ] 5.2.2. Expand `diesel_cte_ext` to cover recursive and non-recursive CTEs
  required by news threading and file hierarchy queries. Acceptance: The crate
  exposes builders validated by unit tests and example code mirroring
  `docs/cte-extension-design.md`. Dependencies: 5.2.1.
- [ ] 5.2.3. Publish API documentation and upgrade guides for `diesel_cte_ext`.
  Acceptance: `cargo doc` renders examples explaining recursive usage and the
  roadmap references the crate for hierarchical queries. Dependencies: 5.2.2.
- [ ] 5.2.4. Add Kani harnesses for `diesel_cte_ext` query builders.
  Acceptance: Kani verifies recursive CTE builders handle empty inputs and
  bounded query shapes without panics. Dependencies: Task “Expand
  `diesel_cte_ext` to cover recursive and non-recursive CTEs required by news
  threading and file hierarchy queries.”

## 6. Quality engineering

### 6.1. Operate continuous fuzzing

- [ ] 6.1.1. Maintain the AFL++ harness in `fuzz/` and regenerate the corpus via
  `make corpus` whenever protocol fields change. Acceptance: Nightly fuzz runs
  complete without harness crashes and store the regenerated corpus artefacts.
  Dependencies: 1.
- [ ] 6.1.2. Ensure `cargo afl fuzz` jobs run in CI using the Docker workflow
  documented in `docs/fuzzing.md`. Acceptance: CI artifacts contain crash
  triage bundles when fuzzing discovers new inputs. Dependencies: 6.1.1.
- [ ] 6.1.3. Establish a triage rota so fuzzing findings are reviewed within two
  working days. Acceptance: Triage logs record ownership, reproduction steps,
  and resolution outcomes for each fuzz discovery. Dependencies: 6.1.2.

### 6.2. Expand automated protocol coverage

- [ ] 6.2.1. Achieve full transaction coverage in integration tests using the
  validator harness. Acceptance: All implemented transactions have at least one
  positive and one negative test case executed in CI. Dependencies: 1.
- [ ] 6.2.2. Add property tests for fragment reassembly, handshake timeouts, and
  privilege bitmaps. Acceptance: Tests fail when framing invariants or
  privilege masks regress and run as part of the default test suite.
  Dependencies: 6.2.1.
- [ ] 6.2.3. Monitor protocol metrics (error codes, fragment counts, retries) to
  catch regressions quickly. Acceptance: Alerts fire when error rates exceed
  thresholds and dashboards expose trend data for release readiness.
  Dependencies: 6.2.2.

### 6.3. Maintain documentation accuracy

- [ ] 6.3.1. Update roadmap cross-references whenever protocol docs, schemas, or
  migration plans change. Acceptance: Documentation audits confirm
  `docs/roadmap.md` and the referenced design documents remain in sync after
  each functional change. Dependencies: 1–5.
- [ ] 6.3.2. Run `markdownlint` and `nixie` across `docs/` as part of CI to
  guarantee style compliance. Acceptance: CI fails on Markdown format issues or
  Mermaid errors and links to remediation guidance. Dependencies: 6.3.1.
- [ ] 6.3.3. Encourage feature owners to add Rustdoc and user-facing manuals
  before closing roadmap tasks. Acceptance: Pull requests closing roadmap items
  include updated API docs and user guidance checked into `docs/`.
  Dependencies: 6.3.2.
