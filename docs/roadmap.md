# Implementation roadmap

This roadmap sequences the work required to deliver a Hotline-compatible server
on top of the `wireframe` transport. It consolidates requirements captured in
<!-- markdownlint-disable-next-line MD013 -->
`docs/design.md`,
`docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`,
`docs/protocol.md`, `docs/file-sharing-design.md`,
`docs/cte-extension-design.md`, `docs/chat-schema.md`, `docs/news-schema.md`,
`docs/fuzzing.md`, and
`docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md`. Items
are organised into phases, steps, and measurable tasks with acceptance criteria
and explicit dependencies. Timeframes are intentionally omitted.

## Phase 1 – Wireframe migration

### Step: Bootstrap the wireframe server

- [x] Task: Extract protocol and domain logic into reusable library modules.
  Acceptance: `mxd` builds as a library crate consumed by both binaries,
  existing integration smoke tests pass unchanged, and core modules remain free
  of `wireframe::*` imports as prescribed in `docs/design.md`. Status:
  Completed on 9 November 2025 by moving the CLI and Tokio runtime into the
  shared `mxd::server` module, so every binary reuses the same entry points.
  Dependencies: None.
- [x] Task: Create the `mxd-wireframe-server` binary that depends on
  `wireframe` and the refactored library. Acceptance: The new binary compiles
  for x86_64 and aarch64 Linux and exposes a minimal listen loop that loads
  configuration. Status: Completed on 18 November 2025 by adding
  `WireframeBootstrap` (`src/server/wireframe.rs`) and a new binary entry point
  so the listener binds after parsing `AppConfig`. Dependencies: Step
  “Bootstrap the wireframe server”.
- [x] Task: Retire the bespoke networking loop once the wireframe pipeline is
  feature-complete. Acceptance: The legacy frame handler is gated behind a
  feature flag or removed without reducing existing automated test coverage,
  aligning with the adapter strategy in
  `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`.
  Status: Completed on 20 November 2025 by gating the legacy runtime behind the
  `legacy-networking` feature, defaulting `server::run()` to Wireframe when
  that feature is disabled, and preserving test coverage via added unit,
  behaviour, and Postgres-backed admin tests. Dependencies: Step “Route
  transactions through wireframe”.

### Step: Implement the wireframe handshake

- [x] Task: Implement the 12-byte Hotline handshake preamble as a
  `wireframe::preamble::Preamble`. Acceptance: Unit tests accept the “TRTP”
  protocol ID and reject malformed inputs outlined in `docs/protocol.md`.
  Status: Completed on 24 November 2025 by introducing `HotlinePreamble` as the
  Wireframe decoder and adding unit and behaviour tests for valid and invalid
  greetings. Dependencies: Step “Bootstrap the wireframe server”.
- [ ] Task: Register success and failure hooks that emit the 8-byte reply and
  enforce a five-second timeout. Acceptance: Handshake errors surface correct
  Hotline error codes and time out idle sockets, matching behaviour documented
  in the migration plan. Dependencies: Task “Implement the 12-byte Hotline
  handshake preamble as a `wireframe::preamble::Preamble`.”
- [ ] Task: Persist handshake metadata (sub-protocol ID, sub-version) into
  per-connection state for later routing decisions. Acceptance: Subsequent
  handlers can branch on the stored metadata to decide compatibility shims.
  Dependencies: Task “Implement the 12-byte Hotline handshake preamble as a
  `wireframe::preamble::Preamble`.”

### Step: Adopt wireframe transaction framing

- [ ] Task: Build a `wireframe` codec that reads and writes the 20-byte
  transaction header and payload framing described in `docs/protocol.md`.
  Acceptance: Property tests cover multi-fragment requests and reject invalid
  length combinations. Dependencies: Step “Implement the wireframe handshake”.
- [ ] Task: Surface a streaming API for large payloads so file transfers and
  news posts can consume fragmented messages incrementally. Acceptance:
  Integration tests for file uploads download payloads over multiple fragments
  without buffer exhaustion. Dependencies: Task “Build a `wireframe` codec that
  reads and writes the 20-byte transaction header and payload framing described
  in `docs/protocol.md`.”
- [ ] Task: Reuse existing parameter encoding helpers within the new codec to
  prevent duplicate implementations. Acceptance: All transactions built through
  the new codec match the byte-for- byte output of the existing encoder for
  shared cases. Dependencies: Task “Build a `wireframe` codec that reads and
  writes the 20-byte transaction header and payload framing described in
  `docs/protocol.md`.”

### Step: Route transactions through wireframe

- [ ] Task: Implement a domain-backed `WireframeProtocol` adapter registered
  via `.with_protocol(...)`. Acceptance: The server initialisation builds the
  adapter described in
  `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`, and
  routing smoke tests exercise every handler through this port without invoking
  legacy wiring. Dependencies: Step “Adopt wireframe transaction framing”.
- [ ] Task: Map every implemented transaction ID to a `wireframe` route that
  delegates to the existing domain handlers. Acceptance: Login, news listing,
  and file listing integration tests run against the wireframe server without
  code duplication. Dependencies: Task “Implement a domain-backed
  `WireframeProtocol` adapter registered via `.with_protocol(...)`.”
- [ ] Task: Introduce a shared session context that tracks the authenticated
  user, privileges, and connection flags. Acceptance: Session state survives
  across handlers and enforces privilege checks defined in `docs/protocol.md`.
  Dependencies: Task “Map every implemented transaction ID to a `wireframe`
  route that delegates to the existing domain handlers.”
- [ ] Task: Provide outbound transport and messaging traits so domain code can
  respond without depending on `wireframe` types. Acceptance: Domain modules
  interact with adapter traits defined alongside the server boundary, and the
  crate continues compiling with no direct `wireframe` imports, matching
  guidance in `docs/design.md`. Dependencies: Task “Map every implemented
  transaction ID to a `wireframe` route that delegates to the existing domain
  handlers.”
- [ ] Task: Provide a reply builder that mirrors Hotline error propagation and
  logging conventions. Acceptance: Error replies retain the original
  transaction IDs and are logged through the existing tracing infrastructure.
  Dependencies: Task “Provide outbound transport and messaging traits so domain
  code can respond without depending on `wireframe` types.”

### Step: Validate Hotline and SynHX compatibility

- [ ] Task: Detect clients that XOR-encode text fields and transparently decode
  or encode responses when required. Acceptance: SynHX parity tests cover
  password, message, and news bodies with the XOR toggle enabled. Dependencies:
  Step “Implement the wireframe handshake”.
- [ ] Task: Gate protocol quirks on the handshake sub-version so Hotline 1.9
  fallbacks remain available. Acceptance: Compatibility tests prove Hotline
  1.8.5, Hotline 1.9, and SynHX all log in, list users, and exchange messages
  successfully. Dependencies: Task “Persist handshake metadata (sub-protocol
  ID, sub- version) into per-connection state for later routing decisions.”
- [ ] Task: Publish an internal compatibility matrix documenting supported
  clients, known deviations, and required toggles. Acceptance: The matrix lives
  in `docs/` and is referenced by release notes during QA sign-off.
  Dependencies: Task “Gate protocol quirks on the handshake sub-version so
  Hotline 1.9 fallbacks remain available.”

### Step: Regression and platform verification

- [ ] Task: Port unit and integration tests so they start the wireframe server
  binary under test. Acceptance: `cargo test` exercises login, presence, file
  listing, and news flows against the new binary. Dependencies: Step “Route
  transactions through wireframe”.
- [ ] Task: Extend the hx-based validator harness to target the wireframe
  server. Acceptance: The harness covers login, file download, chat, and news
  flows and runs in CI. Dependencies: Task “Port unit and integration tests so
  they start the wireframe server binary under test.”
- [ ] Task: Add cross-architecture CI jobs (x86_64 and aarch64 Linux) for the
  wireframe build and smoke tests. Acceptance: CI publishes binaries for both
  targets and reports handshake, login, and shutdown smoke results.
  Dependencies: Task “Port unit and integration tests so they start the
  wireframe server binary under test.”

## Phase 2 – Session and presence parity

### Step: Harden session lifecycle management

- [ ] Task: Implement transactions 300–307 (user list, change notifications,
  agree/disagree flows) exactly as described in `docs/protocol.md`. Acceptance:
  Clients receive Notify Change User (301) and Notify Delete User (302)
  broadcasts that reflect session joins, updates, and logouts. Dependencies:
  Phase “Wireframe migration”.
- [ ] Task: Track presence state, idle timers, and away flags in the shared
  session context. Acceptance: Auto-away and idle timeout thresholds trigger
  updates in the user list without manual refresh. Dependencies: Task
  “Implement transactions 300–307 (user list, change notifications,
  agree/disagree flows) exactly as described in `docs/protocol.md`.”
- [ ] Task: Expose admin tooling to inspect and terminate sessions by user ID
  or connection ID. Acceptance: Administrators can enumerate sessions and
  terminate connections using the CLI, with events mirrored to all clients.
  Dependencies: Task “Track presence state, idle timers, and away flags in the
  shared session context.”

### Step: Implement private messaging workflows

- [ ] Task: Support Send Instant Message (108) and Server Message (104)
  transactions including quoting and automatic responses. Acceptance: Unit
  tests validate option codes 1–4 and quoted replies defined in
  `docs/protocol.md`. Dependencies: Step “Harden session lifecycle management”.
- [ ] Task: Enforce privilege code 19 (Send Private Message) and refusal flags
  surfaced by Set Client User Info (304). Acceptance: Users without privilege
  receive error replies and refusal flags block delivery in integration tests.
  Dependencies: Task “Support Send Instant Message (108) and Server Message
  (104) transactions including quoting and automatic responses.”
- [ ] Task: Log private message metadata (sender, recipient, timestamp, option)
  for auditing without storing message bodies. Acceptance: Audit records
  support tracing abuse reports while respecting privacy requirements.
  Dependencies: Task “Enforce privilege code 19 (Send Private Message) and
  refusal flags surfaced by Set Client User Info (304).”

### Step: Deliver chat room operations

- [ ] Task: Apply the schema in `docs/chat-schema.md` via Diesel migrations and
  model structs. Acceptance: Tables `chat_rooms`, `chat_participants`,
  `chat_messages`, and `chat_invites` exist in SQLite and PostgreSQL
  migrations. Dependencies: Phase “Wireframe migration”.
- [ ] Task: Implement chat transactions (Create Chat 111, Invite 112–114, Join
  115, Leave 116, Send Chat 105, Chat Message 106, Notify Chat events 117–120)
  exactly as specified in `docs/protocol.md`. Acceptance: Multi-user
  integration tests verify room creation, invitations, subjects, and broadcast
  messages. Dependencies: Task “Apply the schema in `docs/chat-schema.md` via
  Diesel migrations and model structs.”
- [ ] Task: Persist room transcripts and enforce retention policies (per-room
  limits or duration-based pruning). Acceptance: Transcript retrieval returns
  chronological chat history within configured limits and purges older rows
  automatically. Dependencies: Task “Implement chat transactions (Create Chat
  111, Invite 112–114, Join 115, Leave 116, Send Chat 105, Chat Message 106,
  Notify Chat events 117–120) exactly as specified in `docs/protocol.md`.”

## Phase 3 – File services parity

### Step: Align file metadata and access control

- [ ] Task: Introduce the `FileNode` schema and permission model described in
  `docs/file-sharing-design.md`. Acceptance: Diesel migrations produce tables
  with folder/file alias support and integrate with the shared permission
  tables. Dependencies: Phase “Wireframe migration”.
- [ ] Task: Migrate existing file metadata into `FileNode` records without data
  loss. Acceptance: All legacy file entries appear in the new schema with
  correct hierarchy, comments, and ACLs. Dependencies: Task “Introduce the
  `FileNode` schema and permission model described in
  `docs/file-sharing-design.md`.”
- [ ] Task: Implement caching or indexing to accelerate frequent directory
  listings. Acceptance: Directory list latency stays within agreed SLAs for
  repositories containing thousands of nodes. Dependencies: Task “Migrate
  existing file metadata into `FileNode` records without data loss.”

### Step: Provide file listing and metadata transactions

- [ ] Task: Implement Get File Name List (200) and Get File Info (206) using
  the new schema. Acceptance: Clients receive Hotline-compliant listing records
  including size, comments, and privilege flags. Dependencies: Step “Align file
  metadata and access control”.
- [ ] Task: Implement Set File Info (207) for renames, comments, and drop box
  flags with privilege enforcement. Acceptance: Integration tests verify
  renames, comment edits, and drop box toggles with proper audit logs.
  Dependencies: Task “Implement Get File Name List (200) and Get File Info
  (206) using the new schema.”
- [ ] Task: Expose folder-specific ACL management commands to admins.
  Acceptance: Admins can grant or revoke folder privileges through CLI or
  administrative transactions with immediate effect. Dependencies: Task
  “Implement Set File Info (207) for renames, comments, and drop box flags with
  privilege enforcement.”

### Step: Build transfer and resume pipelines

- [ ] Task: Implement Download File (202) with resume support and dedicated
  data-channel negotiation. Acceptance: Partial downloads resume correctly
  after reconnect and respect bandwidth throttling policies. Dependencies: Step
  “Provide file listing and metadata transactions”.
- [ ] Task: Implement Upload File (203) and Upload Folder (213) with multipart
  streaming to `object_store` backends. Acceptance: Uploads can be paused and
  resumed and emit progress events to the client. Dependencies: Task “Implement
  Download File (202) with resume support and dedicated data-channel
  negotiation.”
- [ ] Task: Implement Download Folder (210) with recursive packaging and
  optional compression. Acceptance: Clients receive multi-file archives that
  reconstruct the folder hierarchy faithfully. Dependencies: Task “Implement
  Upload File (203) and Upload Folder (213) with multipart streaming to
  `object_store` backends.”

### Step: Deliver advanced file management features

- [ ] Task: Implement folder operations (New Folder 205, Move 208, Delete 204)
  with transactional integrity. Acceptance: Operations either commit fully or
  roll back on failure and honour folder-level privileges. Dependencies: Step
  “Build transfer and resume pipelines”.
- [ ] Task: Implement Make File Alias (209) with validation against cycles and
  permission inheritance. Acceptance: Alias creation respects access
  restrictions and exposes target metadata consistently. Dependencies: Task
  “Implement folder operations (New Folder 205, Move 208, Delete 204) with
  transactional integrity.”
- [ ] Task: Surface drop box behaviours (upload-only folders, per-user mailbox)
  defined in `docs/file-sharing-design.md`. Acceptance: Drop boxes hide
  contents from non-admin users while accepting uploads and notifying
  moderators. Dependencies: Task “Implement folder operations (New Folder 205,
  Move 208, Delete 204) with transactional integrity.”

### Step: Support multi-backend object storage

- [ ] Task: Configure `object_store` drivers for local disk, S3-compatible, and
  Azure Blob targets. Acceptance: Integration tests upload and download files
  across all supported backends using the same code path. Dependencies: Step
  “Build transfer and resume pipelines”.
- [ ] Task: Implement lifecycle hooks for retention policies (expiry, archive,
  delete) per folder. Acceptance: Scheduled jobs archive or delete expired
  content and update listings accordingly. Dependencies: Task “Configure
  `object_store` drivers for local disk, S3-compatible, and Azure Blob targets.”
- [ ] Task: Instrument transfer metrics (latency, throughput, error rate) for
  observability dashboards. Acceptance: Metrics feed Grafana-style dashboards
  and alert on sustained regressions. Dependencies: Task “Implement lifecycle
  hooks for retention policies (expiry, archive, delete) per folder.”

## Phase 4 – News system rebuild

### Step: Align the news schema and migrations

- [ ] Task: Apply the schema from `docs/news-schema.md` (bundles, categories,
  threaded articles, permissions). Acceptance: Schema exists in both SQLite and
  PostgreSQL migrations with referential integrity and required indices.
  Dependencies: Phase “Wireframe migration”.
- [ ] Task: Migrate existing news content into the new structure with bundle
  and category GUIDs. Acceptance: Historical articles retain threading (parent,
  prev, next) and remain addressable by GUID. Dependencies: Task “Apply the
  schema from `docs/news-schema.md` (bundles, categories, threaded articles,
  permissions).”
- [ ] Task: Seed the permissions catalogue with the 38 news privilege codes
  documented in `docs/protocol.md`. Acceptance: Users acquire news privileges
  via `user_permissions` entries and transactions honour those flags.
  Dependencies: Task “Apply the schema from `docs/news-schema.md` (bundles,
  categories, threaded articles, permissions).”

### Step: Implement news browsing transactions

- [ ] Task: Implement Get News Category List, Get News Category, and Get News
  Article transactions with paging support. Acceptance: Clients can traverse
  bundles, categories, and threaded articles with consistent sequence numbers.
  Dependencies: Step “Align the news schema and migrations”.
- [ ] Task: Implement news search and filtering (by poster, date range,
  headline) using Diesel query helpers. Acceptance: Search queries return
  results within 200 ms for typical data sets and support CTE-backed recursive
  traversal where needed. Dependencies: Task “Implement Get News Category List,
  Get News Category, and Get News Article transactions with paging support.”
- [ ] Task: Cache frequently accessed bundles and article headers.
  Acceptance: Cache hit rates exceed 90% for popular bundles without stale data
  exceeding configured TTLs. Dependencies: Task “Implement news search and
  filtering (by poster, date range, headline) using Diesel query helpers.”

### Step: Implement news authoring and moderation

- [ ] Task: Implement Post News, Post News Reply, Edit News, and Delete News
  transactions with full audit trails. Acceptance: Article revisions capture
  editor, timestamp, and diff metadata and enforce privilege codes 21 and 33.
  Dependencies: Step “Align the news schema and migrations”.
- [ ] Task: Implement category and bundle management transactions (create,
  rename, delete) with hierarchical updates. Acceptance: Bundle/category
  operations update GUIDs, sequence numbers, and notify subscribed clients.
  Dependencies: Task “Implement Post News, Post News Reply, Edit News, and
  Delete News transactions with full audit trails.”
- [ ] Task: Provide moderation tooling for locking threads, pinning articles,
  and escalating reports. Acceptance: Moderators can lock or pin via the CLI or
  administrative transactions and clients reflect the state in listings.
  Dependencies: Task “Implement Post News, Post News Reply, Edit News, and
  Delete News transactions with full audit trails.”

## Phase 5 – Administration and database platform

### Step: Complete administrative protocol coverage

- [ ] Task: Implement administrative transactions (Kick User 109, Ban User,
  Broadcast 152, Server Message 104 without user ID) per `docs/protocol.md`.
  Acceptance: Administrators can manage sessions, issue broadcasts, and close
  the server gracefully. Dependencies: Phase “Session and presence parity”.
- [ ] Task: Enforce privilege codes 14–18 and 22–32 across administrative
  handlers. Acceptance: Privilege violations return protocol error codes and
  are logged with user context. Dependencies: Task “Implement administrative
  transactions (Kick User 109, Ban User, Broadcast 152, Server Message 104
  without user ID) per `docs/protocol.md`.”
- [ ] Task: Provide audit logs summarising administrative actions with before/
  after snapshots where applicable. Acceptance: Audit entries include actor,
  target, action, and rationale fields and feed compliance reporting.
  Dependencies: Task “Enforce privilege codes 14–18 and 22–32 across
  administrative handlers.”

### Step: Harden database backends and query tooling

- [ ] Task: Finalise PostgreSQL support, ensuring migrations, Diesel builders,
  and tests run against PostgreSQL 14+. Acceptance: CI runs the full
  integration suite on SQLite and PostgreSQL backends with identical behaviour.
  Dependencies: Phase “File services parity”.
- [ ] Task: Expand `diesel_cte_ext` to cover recursive and non-recursive CTEs
  required by news threading and file hierarchy queries. Acceptance: The crate
  exposes builders validated by unit tests and example code mirroring
  `docs/cte-extension-design.md`. Dependencies: Task “Finalise PostgreSQL
  support, ensuring migrations, Diesel builders, and tests run against
  PostgreSQL 14+.”
- [ ] Task: Publish API documentation and upgrade guides for `diesel_cte_ext`.
  Acceptance: `cargo doc` renders examples explaining recursive usage and the
  roadmap references the crate for hierarchical queries. Dependencies: Task
  “Expand `diesel_cte_ext` to cover recursive and non-recursive CTEs required
  by news threading and file hierarchy queries.”

## Phase 6 – Quality engineering

### Step: Operate continuous fuzzing

- [ ] Task: Maintain the AFL++ harness in `fuzz/` and regenerate the corpus via
  `make corpus` whenever protocol fields change. Acceptance: Nightly fuzz runs
  complete without harness crashes and store the regenerated corpus artefacts.
  Dependencies: Phase “Wireframe migration”.
- [ ] Task: Ensure `cargo afl fuzz` jobs run in CI using the Docker workflow
  documented in `docs/fuzzing.md`. Acceptance: CI artifacts contain crash
  triage bundles when fuzzing discovers new inputs. Dependencies: Task
  “Maintain the AFL++ harness in `fuzz/` and regenerate the corpus via
  `make corpus` whenever protocol fields change.”
- [ ] Task: Establish a triage rota so fuzzing findings are reviewed within two
  working days. Acceptance: Triage logs record ownership, reproduction steps,
  and resolution outcomes for each fuzz discovery. Dependencies: Task “Ensure
  `cargo afl fuzz` jobs run in CI using the Docker workflow documented in
  `docs/fuzzing.md`.”

### Step: Expand automated protocol coverage

- [ ] Task: Achieve full transaction coverage in integration tests using the
  validator harness. Acceptance: All implemented transactions have at least one
  positive and one negative test case executed in CI. Dependencies: Phase
  “Wireframe migration”.
- [ ] Task: Add property tests for fragment reassembly, handshake timeouts, and
  privilege bitmaps. Acceptance: Tests fail when framing invariants or
  privilege masks regress and run as part of the default test suite.
  Dependencies: Task “Achieve full transaction coverage in integration tests
  using the validator harness.”
- [ ] Task: Monitor protocol metrics (error codes, fragment counts, retries) to
  catch regressions quickly. Acceptance: Alerts fire when error rates exceed
  thresholds and dashboards expose trend data for release readiness.
  Dependencies: Task “Add property tests for fragment reassembly, handshake
  timeouts, and privilege bitmaps.”

### Step: Maintain documentation accuracy

- [ ] Task: Update roadmap cross-references whenever protocol docs, schemas, or
  migration plans change. Acceptance: Documentation audits confirm
  `docs/roadmap.md` and the referenced design documents remain in sync after
  each functional change. Dependencies: All prior phases.
- [ ] Task: Run `markdownlint` and `nixie` across `docs/` as part of CI to
  guarantee style compliance. Acceptance: CI fails on Markdown format issues or
  Mermaid errors and links to remediation guidance. Dependencies: Task “Update
  roadmap cross-references whenever protocol docs, schemas, or migration plans
  change.”
- [ ] Task: Encourage feature owners to add Rustdoc and user-facing manuals
  before closing roadmap tasks. Acceptance: Pull requests closing roadmap items
  include updated API docs and user guidance checked into `docs/`.
  Dependencies: Task “Run `markdownlint` and `nixie` across `docs/` as part of
  CI to guarantee style compliance.”
