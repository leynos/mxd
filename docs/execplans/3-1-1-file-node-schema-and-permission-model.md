# Introduce the `FileNode` schema and permission model (roadmap 3.1.1)

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETED

Progress note (2026-04-22):

- Follow-on CI hardening uncovered that a reasoned
  `#[expect(unused_imports)]` on `create_file_node` was still the wrong shape
  for coverage builds. The `postgres` coverage path fulfils the import, which
  turns the expectation itself into a denied warning under
  `-D unfulfilled-lint-expectations`.
- The repository helper now uses compile-time gated imports inside
  `create_file_node` instead of attribute-based lint suppression or
  expectation. This keeps the feature split explicit and avoids policy drift in
  both standard Clippy and coverage-driven builds.
- Follow-up review found that routing `GetFileNameList` straight to
  `list_visible_root_file_nodes_for_user` breaks upgraded databases whose
  additive migrations still only contain legacy `files` and `file_acl` rows.
  Until roadmap work adds a real backfill, the file-list helper must fall back
  to the legacy tables when no visible root `file_nodes` exist.
- Review follow-up also identified two maintainability gaps:
  the seeded `download_file` privilege mapping had drift risk because tests,
  fixtures, and repository helpers all re-spelled the same code/name pair, and
  the migration watchdog timeout needed a deterministic test seam plus an
  override path so non-CI environments can tighten the cap without patching
  source.
- A later verification pass also tightened the roadmap artefacts around this
  work: documentation links are now repository-relative, file-node fixtures use
  the identifiers returned by insertion instead of reloading the whole table,
  and migration DDL now guards against self-parent rows plus stale polymorphic
  ACL principals.
- The current verification pass on the rebased branch found a second set of
  still-live follow-ups: `GetFileNameList` was still dropping legacy-visible
  files whenever any new `file_nodes` ACL rows existed, the migration timeout
  was still bypassing merged config by reading the environment directly inside
  the DB adapter, and `file_path` still lived at the crate root instead of the
  DB adapter boundary.
- The implementation response is to keep the compatibility union explicit at
  the repository edge, thread migration timeout through `AppConfig`, move the
  recursive file-path helper into `src/db/`, and tighten migration DDL so file
  node basenames cannot be empty or contain `/`.

## Purpose / big picture

Roadmap item 3.1.1 replaces the current flat `files` and `file_acl` schema with
the hierarchical `FileNode` model described in `docs/file-sharing-design.md`
and connects file metadata to the project's shared permission model. The goal
is to create a dual-backend Diesel schema that can represent folders, files,
aliases, comments, and drop boxes without painting later file-service tasks
into a corner.

From a hexagonal-architecture perspective, this roadmap step must also define
the persistence-side driven port for file hierarchy and access-control data so
that Diesel, SQL, and recursive query mechanics stay in the outbound adapter
layer instead of leaking into command handlers, protocol routing, or future
application services.

Success is observable when:

- SQLite and PostgreSQL migrations create the `FileNode` hierarchy and the
  shared permission structures required to express file ACLs.
- The schema supports folders, files, aliases, comments, drop box metadata,
  parent-child traversal, and principal-based ACL rows.
- Diesel models, `schema.rs`, and minimal repository helpers compile for both
  backends and are ready for roadmap items 3.1.2 and 3.2.
- `diesel-cte-ext` is used for backend-neutral hierarchical relational logic
  needed to resolve paths and parent/descendant relationships.
- Unit tests written with `rstest` cover happy, unhappy, and edge cases for
  schema invariants and repository helpers.
- Behavioural tests written with `rstest-bdd` cover user-visible flows where
  the new schema is already observable, chiefly the existing file-listing
  transport path.
- Local PostgreSQL validation uses `pg_embedded_setup_unpriv`.
- `docs/design.md` records the final schema and permission decisions,
  `docs/users-guide.md` reflects any user-visible changes, and
  `docs/roadmap.md` marks 3.1.1 done only after all quality gates pass.

## Constraints

- Keep scope bounded to roadmap 3.1.1:
  - schema, Diesel integration, minimal query helpers, and regression-safe
    test updates are in scope;
  - new user-facing file operations from roadmap 3.2 and 3.4 are out of scope.
- Reconcile `docs/file-sharing-design.md` with the existing shared-permission
  direction in `docs/news-schema.md` and `docs/design.md` before writing
  migrations. The implementation must not create two incompatible permission
  systems.
- Treat file hierarchy persistence and ACL lookup as an outbound-adapter
  concern. Domain or application-facing interfaces must own the data shapes and
  semantics; Diesel-specific models and SQL details stay behind that boundary.
- Preserve existing wireframe and legacy file-list behaviour unless a design
  decision explicitly changes it and the change is documented.
- Prefer additive migration steps in 3.1.1. Do not drop legacy `files` or
  `file_acl` tables unless a verified backfill path for roadmap 3.1.2 exists.
- Use `diesel-cte-ext` for hierarchical query helpers rather than ad hoc
  backend-specific SQL branches.
- Do not let protocol or transport details leak into the persistence design.
  Hotline privilege codes may inform ACL semantics, but protocol field parsing,
  transaction framing, and reply shaping remain inbound-adapter concerns.
- Use `rstest` fixtures for unit and integration coverage and `rstest-bdd`
  for behavioural scenarios where there is an observable protocol contract.
- Use `pg_embedded_setup_unpriv` to validate PostgreSQL-backed test flows.
- Keep documentation in en-GB-oxendict spelling and wrap Markdown prose at
  80 columns.
- Do not mark roadmap item 3.1.1 done until migrations, tests, docs, and gates
  are all complete.

## Tolerances (exception triggers)

- Scope: if implementation expands beyond roughly 25 files or 900 net lines,
  stop and re-check whether 3.1.2 or 3.2 work is being pulled in early.
- Schema ambiguity: if the shared permission model cannot be reconciled across
  `docs/file-sharing-design.md`, `docs/news-schema.md`, and current code
  without a substantive architecture choice, pause and document the choice in
  `docs/design.md` before continuing.
- Data migration coupling: if removing or reshaping `files` / `file_acl` would
  make roadmap 3.1.2 materially harder or risk data loss, stop and keep the old
  tables in place for one more roadmap step.
- Behavioural reach: if proving aliases or folders requires net-new protocol
  behaviour not scheduled until 3.2, keep those cases at repository/integration
  level and do not force premature transaction changes.
- Dependency pressure: if this task appears to require a new crate beyond the
  already-approved `diesel-cte-ext` and `pg-embed-setup-unpriv`, stop and
  escalate.
- Rework loop: if the full quality gates fail twice after targeted fixes, stop
  and escalate with captured logs.

## Risks

- Risk: the repository currently has no shared permission tables in source
  migrations, while the file-sharing design and news design assume one.
  Severity: high. Likelihood: high. Mitigation: make schema reconciliation the
  first implementation stage and record the chosen model in `docs/design.md`
  before writing DDL.
- Risk: replacing `files` / `file_acl` too early could block roadmap 3.1.2 data
  migration or break the existing `GetFileNameList` handler. Severity: high.
  Likelihood: medium. Mitigation: add new tables additively, switch helpers
  carefully, and keep legacy structures until the backfill step lands.
- Risk: cross-backend constraints for aliases and folders diverge between
  PostgreSQL and SQLite. Severity: medium. Likelihood: medium. Mitigation: use
  portable `TEXT` + `CHECK` modelling where possible and cover both backends in
  migration and helper tests.
- Risk: recursive path logic becomes backend-specific or duplicated.
  Severity: medium. Likelihood: medium. Mitigation: limit 3.1.1 helper work to
  a small set of `diesel-cte-ext` powered queries shared by both backends.
- Risk: behavioural tests become flaky once PostgreSQL is added to the loop.
  Severity: medium. Likelihood: low. Mitigation: bootstrap clusters through
  `pg_embedded_setup_unpriv`, reuse deterministic fixtures, and keep transport
  scenarios narrow.

## Agent team and ownership

Implementation should be split into explicit workstreams:

- Schema agent:
  reconciles the permission design, authors dual-backend migrations, and keeps
  the DDL portable.
- Persistence agent:
  updates `src/schema.rs`, models, and minimal repository helpers, including
  `diesel-cte-ext` path and hierarchy queries.
- Verification agent:
  owns `rstest` and `rstest-bdd` coverage, PostgreSQL setup, and quality-gate
  execution with `tee`-captured logs.
- Documentation agent:
  updates `docs/design.md`, `docs/file-sharing-design.md` if the implemented
  shape refines the earlier sketch, `docs/users-guide.md`, and
  `docs/roadmap.md`.

Handoff rule: each workstream must leave file-level evidence and test evidence
before the next stage proceeds.

## Hexagonal architecture audit

Audit summary against the `hexagonal-architecture` skill:

- The original plan already scoped the storage work narrowly, but it needed a
  more explicit statement that `FileNode` persistence is a driven port with
  Diesel as the outbound adapter.
- File hierarchy invariants such as node kind legality, alias resolution,
  sibling uniqueness, and ACL semantics should be expressible in
  domain-facing/application-facing terms even when enforced in SQL.
- Command handlers and wireframe routing must consume a narrow file access
  interface or application service rather than reach for Diesel models,
  backend-specific queries, or recursive SQL directly.
- Adapter isolation matters here: object storage, Diesel persistence, and
  protocol handling should coordinate through the core rather than growing
  point-to-point dependencies as roadmap items 3.1.2 and 3.2 land.

## Context and orientation

Current relevant state:

- The current database schema still uses flat file metadata in
  `migrations/postgres/00000000000004_create_files/up.sql`,
  `migrations/sqlite/00000000000004_create_files/up.sql`,
  [src/schema.rs](../../src/schema.rs), and
  [src/models.rs](../../src/models.rs).
- File persistence logic currently lives in
  [src/db/files.rs](../../src/db/files.rs) and only supports `create_file`,
  `add_file_acl`, and `list_files_for_user`.
- The existing file-list transaction handler in
  [src/commands/handlers.rs](../../src/commands/handlers.rs) assumes a flat
  list of files filtered by `file_acl`.
- Test fixtures in
  [test-util/src/fixtures/mod.rs](../../test-util/src/fixtures/mod.rs) and
  transport tests in [tests/file_list.rs](../../tests/file_list.rs) currently
  seed and validate the old schema.
- `docs/file-sharing-design.md` defines the intended `FileNode` model with
  folders, files, aliases, `is_dropbox`, comments, and principal-based ACLs.
- `docs/news-schema.md` and `docs/design.md` describe a normalized shared
  permission direction (`permissions`, `user_permissions`) that is not yet
  present in migrations.
- `docs/verification-strategy.md` explicitly calls out permission mapping and
  predicate logic as Kani candidates later in roadmap 3.1.4, so 3.1.1 should
  shape data and predicates to be verification-friendly.
- `docs/protocol.md` defines the relevant privilege codes, especially file and
  folder privileges 0-8, comment privileges 28-29, drop-box visibility 30, and
  alias creation 31.

Known design tension to resolve up front:

- `docs/file-sharing-design.md` sketches a resource-oriented `Permission` ACL
  table plus `User`, `Group`, and `UserGroup`.
- `docs/news-schema.md` sketches a privilege catalogue
  (`permissions`, `user_permissions`) for protocol privilege codes.
- The implementation for 3.1.1 must either unify these into one coherent shared
  model or document a clean split between global privilege assignment and
  per-resource ACL rows.

## Relevant references and skills

Repository documentation to keep in view while implementing this plan:

- `docs/design.md`: authoritative architecture notes, including current
  hexagonal boundaries and the normalized permission direction.
- `docs/adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`:
  repository-specific interpretation of dependency direction, port ownership,
  and adapter isolation.
- `docs/file-sharing-design.md`: target `FileNode` hierarchy, file metadata,
  alias, drop box, and ACL concepts for roadmap item 3.1.1.
- `docs/news-schema.md`: existing shared-permission catalogue direction that
  must be reconciled with file ACLs.
- `docs/protocol.md`: Hotline privilege codes and file or folder semantics that
  inform the ACL model without dictating adapter internals.
- `docs/verification-strategy.md`: verification boundary guidance, including
  why permission predicates and mapping logic should stay pure and
  verification-friendly.
- `docs/cte-extension-design.md`: recursive-CTE rationale for backend-neutral
  hierarchy traversal using `diesel-cte-ext`.
- `docs/rust-testing-with-rstest-fixtures.md`: fixture and `rstest` patterns
  expected for repository and integration coverage.
- `docs/rstest-bdd-users-guide.md`: behavioural-test structure for scenarios
  that stay observable at the protocol boundary.
- `docs/pg-embed-setup-unpriv-users-guide.md`: local PostgreSQL setup and test
  cluster guidance.
- `docs/developers-guide.md`: current project structure and import guidance for
  adapter-facing modules.
- `docs/documentation-style-guide.md`: documentation update conventions for the
  follow-on `docs/design.md`, `docs/users-guide.md`, and `docs/roadmap.md`
  edits required by this roadmap step.

Repository-level guidance that remains useful while implementing this work:

- `hexagonal-architecture`: use for dependency-rule checks, port placement,
  adapter isolation, and drift detection.
- `rust-types-and-apis`: use when designing repository traits, newtypes, and
  domain-facing interfaces for `FileNode` and permission access.
- `rust-errors`: use when shaping semantic repository or application errors at
  the port boundary.

## Plan of work

### Stage A: reconcile the permission architecture before writing migrations

Audit the current repository state against:

- `docs/file-sharing-design.md`
- `docs/news-schema.md`
- `docs/design.md`
- `docs/protocol.md`

Produce and record a concrete design decision covering:

- which domain or application module owns the driven port for file-node
  persistence and ACL lookup, and which adapter module implements it;
- whether global protocol privileges remain normalized in
  `permissions` / `user_permissions`;
- whether file and folder ACLs live in a dedicated shared resource-permission
  table or a documented equivalent;
- whether `groups` and `user_groups` are introduced now or deferred behind an
  explicitly documented compatibility shim;
- whether `users.global_access` is needed in addition to normalized
  permissions, or whether current session privilege loading remains unchanged
  in 3.1.1.
- how protocol privilege codes map into storage predicates and ACL semantics
  without forcing protocol-specific types into the persistence layer.

Required outcome:

- `docs/design.md` states the final permission architecture and how file ACLs
  integrate with shared permissions.
- The plan identifies a stable port boundary that later roadmap items can reuse
  without exposing Diesel types or backend-specific recursion details outside
  the adapter layer.

Validation gate for Stage A:

- no unresolved contradiction remains between the file-sharing design and the
  news/shared-permission direction.

### Stage B: add dual-backend schema for `FileNode` and shared permissions

Author new SQLite and PostgreSQL migrations that add the new structures without
destroying current data needed for roadmap 3.1.2. The preferred shape is:

- `file_nodes` table with:
  - stable primary key;
  - node kind (`file`, `folder`, `alias`);
  - `name`;
  - `parent_id`;
  - `alias_target_id`;
  - `object_key`;
  - `size`;
  - `comment`;
  - `is_dropbox`;
  - creation/update timestamps;
  - creator reference.
- the shared principal and permission tables chosen in Stage A.
- indices and constraints for:
  - unique sibling names;
  - fast parent-child listing;
  - alias lookup;
  - resource-principal uniqueness in ACL rows;
  - referential integrity for parents, creators, targets, and principals.

Schema-level invariant checks should cover at least:

- folders cannot carry an `object_key`;
- aliases cannot carry stored file data of their own;
- files cannot point at `alias_target_id`;
- alias targets cannot create trivial self-reference;
- name uniqueness is enforced per parent, not globally.

Required outcome:

- migrations apply cleanly on SQLite and PostgreSQL and Diesel schema
  generation compiles.

Validation gate for Stage B:

- fresh databases on both backends can run all migrations with no manual
  intervention;
- legacy `files` / `file_acl` data remains available for the subsequent
  backfill task unless the migration plan proves a safe in-place carry-forward.

### Stage C: update Diesel schema, models, and minimal persistence helpers

Update the Rust persistence surface so the new schema is representable and
testable:

- refresh `src/schema.rs`;
- add or replace adapter-layer models for `FileNode`, alias metadata, groups,
  shared permission rows, and any helper enums or newtypes required to keep
  code readable;
- define or refine the domain-facing/application-facing repository interface
  that owns file hierarchy and ACL queries;
- keep module-level `//!` comments and Rustdoc current;
- replace file-helper functions that assume a flat file list with a minimal set
  of repository APIs suited to 3.1.1.

The helper surface should stay narrow. It should support only what 3.1.1 needs:

- create seed folders, files, aliases, and ACL rows for tests;
- resolve top-level or path-based nodes;
- query children of a folder;
- resolve alias targets;
- answer "what nodes are visible to this principal?" for the current file-list
  regression path.

Do not implement the full 3.2 transaction surface here.

Validation gate for Stage C:

- all code compiles for SQLite, PostgreSQL, and wireframe-only feature sets;
- repository helpers expose enough functionality to seed and query the new
  schema without ad hoc SQL in tests.
- no Diesel-specific types, backend-specific SQL strings, or recursive-query
  implementation details leak into command handlers, wireframe routing, or
  other non-adapter modules.

### Stage D: add hierarchical query helpers with `diesel-cte-ext`

Use the published `diesel-cte-ext` crate to add backend-neutral recursive query
helpers needed for file hierarchy logic. At minimum, implement and test:

- path resolution from root to a node by parent/name traversal;
- descendant or ancestor traversal needed for hierarchy-aware validation;
- alias-target lookup that can be reused by future file operations.

Keep the helper set deliberately small and aligned to future roadmap tasks. The
purpose of 3.1.1 is to establish the relational foundation, not to build every
future file command early.

Validation gate for Stage D:

- at least one repository helper that matters to `FileNode` hierarchy is backed
  by `diesel-cte-ext` and exercised on both backends.

### Stage E: migrate tests to the new schema with `rstest`

Add focused unit and integration coverage using `rstest`. Cover happy, unhappy,
and edge paths such as:

- creating folder, file, and alias nodes with valid shapes;
- rejecting invalid node-kind combinations;
- enforcing unique sibling names while allowing the same name under different
  parents;
- resolving nested paths correctly;
- joining ACL rows to users or groups according to the chosen shared model;
- preserving drop-box metadata;
- alias rows resolving to their targets;
- idempotent or duplicate ACL insertion behaviour, if the repository API
  preserves that contract.

Update shared test fixtures so file data is seeded through `file_nodes` and the
new permission tables rather than `files` / `file_acl`.

PostgreSQL-backed tests should use `pg_embedded_setup_unpriv` or the crate's
test-cluster API as documented in `docs/pg-embed-setup-unpriv-users-guide.md`.

Validation gate for Stage E:

- repository tests pass on both SQLite and PostgreSQL without backend-specific
  skips other than documented environment limitations.

### Stage F: add behavioural regression coverage with `rstest-bdd`

Only add behavioural scenarios where the schema change is already observable
through an existing server contract. The primary candidate is the current
`GetFileNameList` flow:

- happy: an authenticated user still lists accessible file nodes through the
  existing transport path;
- unhappy: unauthenticated or unauthorized access still fails as before;
- edge: invalid file-list payload handling remains unchanged after the
  persistence rewrite.

Folders, aliases, comments, and drop-box semantics that do not yet have
protocol support in the current branch should stay covered at repository or
integration level until roadmap 3.2 exposes them.

Validation gate for Stage F:

- BDD scenarios prove the old user-visible contract still works when backed by
  the new schema.

### Stage G: documentation and roadmap synchronization

Update:

- `docs/design.md` with the implemented schema and permission decisions;
- `docs/file-sharing-design.md` if the implementation sharpens or supersedes
  the earlier conceptual sketch;
- `docs/users-guide.md` with any user-visible changes, or an explicit note that
  this step is schema-only and preserves current behaviour;
- `docs/roadmap.md` to mark 3.1.1 done only after all implementation and
  verification work is complete.

Required outcome:

- documentation matches the delivered schema, and the roadmap entry has a dated
  completion note.

### Stage H: verification and quality gates

Run local PostgreSQL setup first, then all applicable gates with `tee` and
`set -o pipefail`.

Recommended sequence:

1. Prepare PostgreSQL:

   ```sh
   set -o pipefail
   pg_embedded_setup_unpriv \
     | tee /tmp/pg-setup-$(basename "$PWD")-$(git branch --show-current).out
   ```

2. Format sources:

   ```sh
   set -o pipefail
   make fmt \
     | tee /tmp/fmt-$(basename "$PWD")-$(git branch --show-current).out
   ```

3. Verify Rust formatting:

   ```sh
   set -o pipefail
   make check-fmt \
     | tee /tmp/check-fmt-$(basename "$PWD")-$(git branch --show-current).out
   ```

4. Run type checks:

   ```sh
   set -o pipefail
   make typecheck \
     | tee /tmp/typecheck-$(basename "$PWD")-$(git branch --show-current).out
   ```

5. Run lint checks:

   ```sh
   set -o pipefail
   make lint \
     | tee /tmp/lint-$(basename "$PWD")-$(git branch --show-current).out
   ```

6. Run test suites:

   ```sh
   set -o pipefail
   make test \
     | tee /tmp/test-$(basename "$PWD")-$(git branch --show-current).out
   ```

7. Lint Markdown:

   ```sh
   set -o pipefail
   make markdownlint \
     | tee /tmp/markdownlint-$(basename "$PWD")-$(git branch --show-current).out
   ```

8. Validate Mermaid diagrams:

   ```sh
   set -o pipefail
   make nixie \
     | tee /tmp/nixie-$(basename "$PWD")-$(git branch --show-current).out
   ```

If a gate must be skipped, record the exact reason and the evidence in this
ExecPlan before closing the task.

Architecture-specific checks to include during review:

1. Check dependency direction:

   ```sh
   rg -n "wireframe::|tokio::net|object_store::|diesel::" src
   ```

   Expectation: any new persistence-specific `diesel::*` usage introduced by
   3.1.1 stays inside adapter-facing modules rather than spreading into command
   handlers or protocol routing.

## Concrete implementation checklist

1. Reconcile the shared permission architecture and record the decision in
   `docs/design.md`.
2. Record the file-hierarchy driven-port boundary and the adapter module that
   implements it.
3. Add dual-backend migrations for `file_nodes` and the selected shared
   permission structures.
4. Keep or bridge legacy `files` / `file_acl` tables so roadmap 3.1.2 can
   migrate data safely.
5. Refresh Diesel schema and Rust models.
6. Add or refine narrow repository helpers and port interfaces for hierarchy
   and ACL queries.
7. Implement `diesel-cte-ext` powered path or hierarchy traversal helpers.
8. Update fixtures and `rstest` coverage for happy, unhappy, and edge cases.
9. Update `rstest-bdd` scenarios for existing observable file-list behaviour.
10. Update `docs/file-sharing-design.md`, `docs/design.md`, and
   `docs/users-guide.md` as required.
11. Run PostgreSQL setup and all quality gates with captured logs.
12. Mark roadmap item 3.1.1 done only after all gates pass.

## Progress

- [x] (2026-04-12 00:00Z) Reviewed roadmap item 3.1.1, the file-sharing design,
  the current file schema, and the repository's existing ExecPlan structure.
- [x] (2026-04-12 00:00Z) Confirmed the current implementation still uses flat
  `files` and `file_acl` tables and that no shared permission tables exist in
  source migrations yet.
- [x] (2026-04-12 00:00Z) Confirmed `diesel-cte-ext` and
  `pg_embedded_setup_unpriv` are the intended tools for hierarchical queries
  and local PostgreSQL validation.
- [x] (2026-04-12 00:00Z) Drafted this ExecPlan at the requested path.
- [x] (2026-04-20 08:40Z) Audited the plan against the
  `hexagonal-architecture` skill and tightened the persistence boundary so
  `FileNode` and ACL work is explicitly treated as driven-port plus
  outbound-adapter work.
- [x] (2026-04-20 08:40Z) Added signposts to the repository documentation and
  session-available skills that are most relevant to implementing 3.1.1.
- [x] (2026-04-20 11:20Z) Reconciled the immediate 3.1.1 permission-model
  shape against the current codebase and selected an additive implementation
  path: introduce `permissions`, `user_permissions`, `groups`, `user_groups`,
  `resource_permissions`, and `file_nodes`, while leaving session privilege
  loading on `Privileges::default_user()` until the later auth-focused roadmap
  item lands.
- [x] (2026-04-20 16:05Z) Added dual-backend additive migrations for
  `permissions`, `user_permissions`, `groups`, `user_groups`,
  `resource_permissions`, and `file_nodes`, keeping legacy `files` and
  `file_acl` in place for roadmap item 3.1.2.
- [x] (2026-04-20 16:05Z) Refreshed Diesel schema and models, introduced the
  `mxd::db::file_path` recursive CTE helper, and narrowed the new persistence
  surface in `mxd::db::files` to path resolution, child listing, alias
  resolution, and visible-root listing.
- [x] (2026-04-20 16:05Z) Updated test fixtures, repository coverage, and the
  existing file-list transport path to seed and query `file_nodes` through the
  new ACL model.
- [x] (2026-04-20 16:05Z) Verified the Rust-focused quality gates after the
  schema rewrite: `pg_embedded_setup_unpriv`, `make fmt`, `make check-fmt`,
  `make typecheck`, `make lint`, and `make test`.
- [x] (2026-04-20 18:10Z) Synchronized `docs/design.md`,
  `docs/file-sharing-design.md`, and `docs/users-guide.md` with the delivered
  additive `file_nodes` plus shared-permission schema.
- [x] (2026-04-20 18:10Z) Marked roadmap item 3.1.1 complete after the
  implementation, verification, and documentation work landed together.
- [x] (2026-04-20 18:10Z) Verified the documentation gates:
  `make markdownlint` and `make nixie`.
- [x] (2026-04-22 09:10Z) Verified the rebased branch against the latest review
  findings, fixing only the items still live in the current tree.
- [x] (2026-04-22 09:10Z) Revalidated the final tree with `make check-fmt`,
  `make typecheck`, `make lint`, and `make test` after the follow-up fixes.

## Surprises & Discoveries

- The current codebase still has only `files` and `file_acl`; no shared
  `permissions`, `user_permissions`, `groups`, or `user_groups` migrations
  exist yet.
- The request referenced `docs/pg-embedded-setup-unpriv-users-guide.md`, while
  the repository path is `docs/pg-embed-setup-unpriv-users-guide.md`.
- The file-sharing design's resource-oriented ACL table and the news design's
  normalized privilege catalogue are not yet reconciled in code, and 3.1.1 is
  the first roadmap step that must resolve the mismatch.
- Behavioural testing is only partially applicable to 3.1.1 because aliases,
  comments, folder metadata, and drop-box semantics do not yet have full
  protocol handlers in the current branch.
- The earlier draft implicitly assumed the persistence boundary, but it did not
  state port ownership or adapter isolation explicitly enough for a
  hexagonal-architecture audit.
- A polymorphic ACL row shape (`resource_type` plus `resource_id`,
  `principal_type` plus `principal_id`) stays portable across SQLite and
  PostgreSQL, but portable conditional foreign keys for those polymorphic
  references do not. A strict fully normalized alternative would require split
  ACL tables or backend-specific trigger logic, which is wider than 3.1.1.
- The additive migration set made the SQLite test bootstrap slower under
  `cargo nextest` than the earlier schema, so the migration timeout in
  `src/db/migrations.rs` needed to increase from five seconds to fifteen
  seconds to keep parallel test-database setup stable.
- The repository-level lint suite treats `tokio::select!` macro expansion as a
  banned `%` remainder use through lint expansion, so the migration watchdog
  needed a `futures_util::future::select` implementation instead of the more
  obvious Tokio macro.
- Tightening the fixture helper in `tests/file_nodes_repository.rs` exposed that
  `build_test_db` intentionally returns `None` when the postgres-backed
  integration fixture is unavailable. The test module now gates both backends,
  so the helper must keep that `None` path as a deliberate skip for unavailable
  fixtures instead of assuming SQLite-only execution or turning unsupported
  environments into false failures.

## Decision Log

- Decision: treat 3.1.1 as an additive schema step first and a destructive
  replacement step later. Rationale: roadmap 3.1.2 explicitly requires a
  migration of existing file metadata, so 3.1.1 should not destroy the source
  tables needed for that move. Date/Author: 2026-04-12 / Codex.
- Decision: keep `diesel-cte-ext` usage narrow and foundational in 3.1.1.
  Rationale: the task is about schema and permission modelling, so only the
  hierarchy helpers needed to validate that model should land now. More
  user-facing file operations remain scheduled in roadmap 3.2 and 3.4.
  Date/Author: 2026-04-12 / Codex.
- Decision: require both `rstest` and `rstest-bdd` coverage, but only where the
  latter maps to an existing observable transport contract. Rationale: the user
  requested behavioural coverage where applicable, and forcing protocol work
  ahead of roadmap 3.2 would widen scope unnecessarily. Date/Author: 2026-04-12
  / Codex.
- Decision: treat file hierarchy persistence and ACL lookup as a driven port
  implemented by the Diesel adapter rather than as an extension of command
  handlers. Rationale: this keeps all dependencies pointing inward, prevents
  recursive SQL and backend-specific types from leaking past the adapter
  boundary, and matches the repository's hexagonal architecture guidance.
  Date/Author: 2026-04-20 / Codex.
- Decision: keep global protocol permissions normalized in `permissions` and
  `user_permissions`, introduce `groups` and `user_groups` now, and store
  file-resource ACLs in a separate `resource_permissions` table keyed by
  `permission_id`. Rationale: this keeps one shared privilege catalogue,
  preserves room for group-based ACLs, and lets the existing file-list flow
  move to `file_nodes` without pulling per-session privilege loading or later
  news handlers into 3.1.1. Date/Author: 2026-04-20 / Codex.
- Decision: do not add `users.global_access` in 3.1.1 and do not switch login
  to load `user_permissions` yet. Rationale: session privilege loading is
  already tracked as later work in the auth roadmap, and changing it here would
  widen the blast radius beyond the schema-and-persistence scope of 3.1.1.
  Date/Author: 2026-04-20 / Codex.
- Decision: keep `resource_permissions` polymorphic for this roadmap step even
  though that means principal and resource referential integrity cannot be
  enforced with portable conditional foreign keys on both backends. Rationale:
  it preserves the shared-ACL shape described in the design docs and keeps the
  migration additive; repository helpers and tests will enforce the existence
  and legal combinations of principals and resources until a later roadmap step
  decides whether to split the ACL table or add trigger-backed validation.
  Date/Author: 2026-04-20 / Codex.
- Decision: keep the upgraded-database compatibility path as a union of modern
  `file_nodes` visibility and legacy `files` plus `file_acl` visibility rather
  than a pure fallback only when the modern result set is empty. Rationale:
  partial backfill states can legitimately expose rows from both sources, so
  the repository helper must preserve legacy-visible files until roadmap 3.1.2
  retires them. Date/Author: 2026-04-22 / Codex.
- Decision: thread the migration timeout through merged application config
  instead of letting `src/db/migrations.rs` read the environment directly.
  Rationale: timeout policy belongs at the configuration boundary, and keeping
  the DB adapter on resolved values preserves hexagonal dependency direction.
  Date/Author: 2026-04-22 / Codex.
- Decision: move `file_path` under `src/db/` rather than keeping it at the
  crate root. Rationale: the helper is a recursive Diesel plus CTE adapter
  concern, not a core domain or application module, so its ownership belongs to
  the outbound database adapter. Date/Author: 2026-04-22 / Codex.

## Outcomes & Retrospective

Intended outcomes once implemented:

- The project has a durable hierarchical file schema that can represent
  folders, files, aliases, comments, and drop-box metadata on SQLite and
  PostgreSQL.
- File ACLs are expressed through the same shared permission architecture that
  later roadmap steps can reuse, rather than through a one-off file-only
  mechanism.
- The current file-list behaviour continues to work while the persistence layer
  is modernized underneath it.
- Roadmap items 3.1.2 and 3.2 can build on repository helpers and schema
  invariants established here instead of re-litigating table shape.

- Implemented: additive dual-backend file-sharing schema, recursive path
  helpers built on `diesel-cte-ext`, repository helpers for `file_nodes` and
  ACL lookups, and fixture plus transport wiring so the current file-list flow
  reads from the new tables.
- Verified: `pg_embedded_setup_unpriv`, `make fmt`, `make check-fmt`,
  `make typecheck`, `make lint`, `make test`, `make markdownlint`, and
  `make nixie` all passed for the delivered change set.
- Documentation updated: `docs/design.md`,
  `docs/file-sharing-design.md`, `docs/users-guide.md`, and `docs/roadmap.md`
  now describe the shipped additive schema, the shared-permission split, the
  unchanged user-facing file-list contract, and the roadmap completion state.
- Behavioural coverage: the existing file-list transport regression coverage
  remained intact while fixtures were moved to `file_nodes`, so the observable
  contract stayed exercised without widening 3.1.1 into new protocol work.
- Follow-up hardening after review kept the original scope intact while
  tightening upgrade compatibility, configuration boundaries, migration DDL,
  and fixture structure. The final branch now preserves legacy file visibility
  until backfill exists, keeps migration timeout policy outside the DB adapter,
  and records the rebased implementation decisions in the docs.
- Follow-up work pushed to later roadmap items: session privilege loading from
  `user_permissions`, the `files`/`file_acl` backfill and retirement step in
  3.1.2, drop-box and alias protocol operations in 3.2+, and any stricter
  polymorphic ACL referential integrity beyond portable foreign keys.
