# Introduce the `FileNode` schema and permission model (roadmap 3.1.1)

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: DONE

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 3.1.1 replaces the legacy flat `files` and `file_acl` schema with
the hierarchical `FileNode` model described in `docs/file-sharing-design.md`.
After this work, the database layer will be able to represent folders, files,
aliases, comments, drop boxes, ancestry, and resource-scoped permissions in a
form that both SQLite and PostgreSQL can migrate and query consistently.

Success is observable when:

- Diesel migrations for both backends add the new file-tree tables and any
  required shared-permission tables without breaking existing migrations;
- the new schema supports folder and file aliases, parent-child hierarchy, and
  permission records that can be consumed by shared privilege logic;
- legacy `files` and `file_acl` remain available long enough for roadmap item
  3.1.2 to migrate data safely;
- `rstest` coverage validates schema helpers, migration invariants, and
  recursive query helpers on happy, unhappy, and edge paths;
- `rstest-bdd` coverage exists where behaviour is observable at the repository
  or service boundary, even before full file protocol transactions land in
  roadmap 3.2;
- local PostgreSQL validation uses `pg-embed-setup-unpriv`;
- design decisions are recorded in `docs/file-sharing-design.md` and
  `docs/design.md` where cross-cutting architecture changes need to be made
  explicit;
- `docs/users-guide.md` is updated if this step introduces any user-visible or
  operator-visible behaviour change; and
- `docs/roadmap.md` item 3.1.1 is marked done only after implementation and
  all quality gates pass.

## Constraints

- Keep this step additive. Roadmap 3.1.2 depends on the old tables still being
  present so legacy file metadata can be migrated into the new schema without
  data loss.
- Preserve dual-backend parity. SQLite and PostgreSQL migrations must describe
  the same logical schema, constraints, and indices.
- Use `diesel-cte-ext` for recursive hierarchy operations rather than bespoke
  SQL string assembly or backend-specific ad hoc recursion helpers.
- Use `pg-embed-setup-unpriv` for local PostgreSQL-backed validation before the
  full repository gates.
- Prefer Diesel-portable representations over backend-only types. In practice,
  that means text columns plus `CHECK` constraints for discriminators rather
  than PostgreSQL-only enums.
- Keep module and file size under repository limits; split new database and
  query helpers into focused modules rather than growing `src/db` or
  `src/models.rs` into monoliths.
- Keep documentation in en-GB-oxendict spelling and wrap prose at 80 columns.
- Use `rstest` for unit and integration-style repository tests, and
  `rstest-bdd` where a behaviour can be described cleanly at a repository or
  service boundary.
- Do not mark roadmap item 3.1.1 done until `make check-fmt`, `make lint`,
  `make test`, `make markdownlint`, and `make nixie` pass.

## Tolerances (exception triggers)

- Scope: if implementation exceeds 22 changed files or 900 net lines before
  3.1.2 starts, pause and reassess the split between 3.1.1 and 3.1.2.
- Schema ambiguity: if the shared permission model cannot be reconciled between
  `docs/file-sharing-design.md`, `docs/news-schema.md`, and `docs/design.md`
  without inventing new concepts, stop and document the options before coding.
- Compatibility: if preserving the legacy `files` and `file_acl` tables proves
  impossible alongside the new schema, stop and escalate rather than collapsing
  3.1.1 and 3.1.2 together.
- Dependencies: if a new crate beyond the already published
  `diesel-cte-ext` and `pg-embed-setup-unpriv` is required, stop and escalate.
- Rework loop: if migrations or full gates fail twice after targeted fixes,
  stop and capture logs before proceeding.

## Risks

- Risk: the design documents describe two overlapping permission models
  (`permissions`/`user_permissions` for shared privileges, and a polymorphic
  ACL table for file resources). Severity: high. Likelihood: medium.
  Mitigation: resolve the canonical shared-permission shape first and record it
  in the design docs before touching migrations.
- Risk: root-level uniqueness is easy to get wrong because
  `UNIQUE(parent_id, name)` does not prevent duplicate names when
  `parent_id IS NULL`. Severity: high. Likelihood: high. Mitigation: adopt an
  explicit root-node strategy or an equivalent cross-backend invariant and test
  it on both backends.
- Risk: alias rows can drift into invalid states (missing target, targeting
  deleted nodes, or carrying file-only fields). Severity: high. Likelihood:
  medium. Mitigation: model alias/file/folder invariants with explicit
  constraints and targeted tests for invalid inserts and updates.
- Risk: additive migrations may leave the runtime in a partially upgraded state
  if schema and `schema.rs` updates are incomplete. Severity: medium.
  Likelihood: medium. Mitigation: land migrations, generated schema changes,
  models, and repository tests in one atomic change.
- Risk: behavioural testing could become artificial if forced through protocol
  surfaces that do not yet use the new schema. Severity: medium. Likelihood:
  medium. Mitigation: keep BDD focused on repository/service behaviour for this
  step, and defer wire-level flows to roadmap 3.2.

## Progress

- [x] (2026-04-11) Audited roadmap item 3.1.1, the current Diesel schema, and
      the file-sharing design documents.
- [x] (2026-04-11) Confirmed the current implementation still uses legacy
      `files` and `file_acl` tables, while `diesel-cte-ext` and
      `pg-embed-setup-unpriv` are already published workspace dependencies.
- [x] (2026-04-13) Reconciled the canonical shared-permission schema:
      `permissions`/`user_permissions` remain the global privilege catalogue,
      additive `resource_permissions` rows carry file-node ACL grants, group
      principals are deferred, and a sentinel root folder enforces top-level
      uniqueness portably.
- [x] (2026-04-13) Recorded the permission/root-node decision in
      `docs/file-sharing-design.md` and `docs/design.md`.
- [x] (2026-04-13) Added additive migrations for `file_nodes`,
      `permissions`, `user_permissions`, and `resource_permissions` in both
      `migrations/sqlite/` and `migrations/postgres/`.
- [x] (2026-04-13) Updated Diesel schema/model code and introduced a focused
      `src/db/file_nodes/` query module backed by `diesel-cte-ext`.
- [x] (2026-04-13) Added `rstest` coverage for schema invariants, recursive
      queries, and invalid states on both backends.
- [x] (2026-04-13) Added `rstest-bdd` scenarios for repository behaviours where
      the new schema is observable without waiting for roadmap 3.2 protocol
      work.
- [x] (2026-04-13) Updated documentation, reviewed `docs/users-guide.md` as
      internal-only for this step, and marked `docs/roadmap.md` item 3.1.1
      done after all gates passed.
- [x] (2026-04-13) Ran the full quality gates with `tee` and `set -o pipefail`.

## Surprises & Discoveries

- The repository already depends on `diesel-cte-ext = "0.1.0"` and
  `pg-embed-setup-unpriv = "0.5.0"`, so this task is integration work rather
  than a dependency-adoption task.
- The current database schema has no shared permission tables yet; only the
  legacy `file_acl` join table exists for files.
- `docs/news-schema.md` and `docs/design.md` describe a shared
  `permissions`/`user_permissions` model, while `docs/file-sharing-design.md`
  sketches a polymorphic resource ACL table. This mismatch must be resolved
  deliberately.
- Because roadmap item 3.1.2 performs data migration later, roadmap 3.1.1
  should not drop or rename the legacy file tables.
- The runtime already models Hotline privileges as a `u64` bitflag set in
  `src/privileges.rs`, so resource-scoped ACL rows should store privilege
  bitmasks rather than introducing a second per-resource join table shape.
- `mdformat-all` shells out to `fd`, and `make lint` shells out to
  `whitaker`; both binaries were absent from the base environment and had to be
  installed before the required repository gates would run cleanly.
- PostgreSQL-backed validation required installing both
  `pg_embedded_setup_unpriv` and the companion `pg_worker` binary, then
  exporting `PG_EMBEDDED_WORKER` so the repository test harness could boot the
  embedded server reliably.

## Decision Log

- Decision: roadmap 3.1.1 remains additive and does not remove `files` or
  `file_acl`. Rationale: roadmap 3.1.2 needs those tables as the source of
  truth for data migration. Date/Author: 2026-04-11 / Codex.
- Decision: use a portable discriminator strategy (`TEXT` +
  `CHECK` constraints) for node and principal kinds. Rationale: the repository
  must support SQLite and PostgreSQL through one Diesel model surface.
  Date/Author: 2026-04-11 / Codex.
- Decision: behavioural tests for 3.1.1 should target repository/service
  behaviours rather than wire protocol flows unless a real user-visible
  behaviour is introduced in this step. Rationale: protocol transactions do not
  switch to the new schema until roadmap 3.2. Date/Author: 2026-04-11 / Codex.
- Decision: shared global privileges stay in `permissions` and
  `user_permissions`, while file-resource grants live in an additive
  `resource_permissions` table keyed by `resource_type`, `resource_id`,
  `principal_type`, and `principal_id`, plus a privilege bitmask. Rationale:
  this keeps the broader cross-domain permission catalogue while matching the
  existing `Privileges` bit definitions used by the runtime. Date/Author:
  2026-04-13 / Codex.
- Decision: roadmap 3.1.1 constrains `resource_permissions.principal_type` to
  `user` and defers group principals. Rationale: the repository does not yet
  have `groups` or `user_groups` migrations, and adding them here would widen
  the step beyond the file-node schema cut. Date/Author: 2026-04-13 / Codex.
- Decision: enforce top-level uniqueness with a sentinel root `file_nodes`
  folder inserted by migration. Rationale: routing all top-level children
  through one parent avoids `NULL` uniqueness edge cases across SQLite and
  PostgreSQL while simplifying recursive traversal. Date/Author: 2026-04-13 /
  Codex.

## Outcomes & Retrospective

Intended outcomes once implemented:

- The repository has a durable hierarchical file schema ready for roadmap 3.2
  file listing and metadata transactions.
- The permission model stops being file-only and aligns with the broader
  cross-domain access-control design.
- Recursive ancestry and descendant queries have one supported implementation
  path based on `diesel-cte-ext`.

Retrospective:

- Implemented: additive SQLite and PostgreSQL migrations for
  `file_nodes`, `permissions`, `user_permissions`, and `resource_permissions`;
  Diesel schema/models for the new tables; and a dedicated `src/db/file_nodes/`
  repository module covering root lookup, child lookup, explicit grants, and
  recursive descendant traversal.
- Verified: targeted SQLite and PostgreSQL repository tests for the new module;
  full repository gates via `make fmt`, `make check-fmt`, `make lint`,
  `make test`, `make markdownlint`, and `make nixie`, all captured with `tee`
  and `set -o pipefail`.
- Documentation updated: `docs/file-sharing-design.md`,
  `docs/design.md`, this ExecPlan, and `docs/roadmap.md`. `docs/users-guide.md`
  was reviewed and left unchanged because the step is internal-only.
- Lessons: keeping the legacy file tables in place while adding the sentinel
  root node avoided accidental coupling with roadmap 3.1.2, and constraining
  resource ACLs to user principals kept the schema cut aligned with current
  domain boundaries instead of introducing premature group management.

## Context and orientation

Current relevant state:

- `src/schema.rs` defines only `files` and `file_acl` for file storage.
- `src/models.rs` defines `FileEntry` and `NewFileAcl`, but no hierarchical
  file-tree or shared-permission models.
- `migrations/sqlite/00000000000004_create_files/` and
  `migrations/postgres/00000000000004_create_files/` create the flat file
  schema now used by integration tests.
- `tests/file_list.rs` exercises file listing against the legacy schema and
  therefore should remain stable until roadmap 3.2 migrates handlers.
- `docs/file-sharing-design.md` is the detailed target design for hierarchy,
  aliases, drop boxes, and ACLs.
- `docs/news-schema.md` and `docs/design.md` describe the existing
  cross-domain direction for permissions and must be reconciled with the file
  design before implementation.
- `docs/cte-extension-design.md` documents the expected use of
  `diesel-cte-ext` for recursive queries.
- `docs/pg-embed-setup-unpriv-users-guide.md`,
  `docs/rust-testing-with-rstest-fixtures.md`,
  `docs/rstest-bdd-users-guide.md`, and
  `docs/reliable-testing-in-rust-via-dependency-injection.md` describe the
  preferred test style for deterministic local validation.

Likely implementation touch points:

- `migrations/sqlite/*` and `migrations/postgres/*` for additive schema work.
- `src/schema.rs` and `src/models.rs` for Diesel model updates.
- `src/db/` for a new file-tree repository/query module, ideally separated from
  legacy file-list helpers.
- `test-util/` if shared Postgres-backed or repository fixtures are needed for
  recursive query tests.
- `docs/file-sharing-design.md`, `docs/design.md`, `docs/users-guide.md`, and
  `docs/roadmap.md` for the required documentation updates.

## Plan of work

### Stage A: reconcile the schema contract before coding

- Compare the file-sharing design with the shared permission design already
  described in `docs/news-schema.md` and `docs/design.md`.
- Choose the canonical shared-permission model for this repository and record
  the decision:
  - whether shared global privileges are represented by `permissions` and
    `user_permissions`,
  - whether file resource ACLs live in a separate table linked to principals,
    and
  - whether group principals are in scope now or explicitly deferred.
- Decide the root-node strategy that preserves sibling-name uniqueness on both
  backends.
- Record the chosen design in `docs/file-sharing-design.md` and add a concise
  cross-cutting note to `docs/design.md` if the change affects system-wide
  architecture.

Exit criteria:

- the design docs no longer disagree about the permission model;
- the root-node and alias invariants are explicit; and
- the migration plan remains additive with respect to the legacy file tables.

### Stage B: add additive Diesel migrations for both backends

- Add a new migration pair for SQLite and PostgreSQL that introduces the new
  schema without removing the legacy tables.
- The new schema should cover at least:
  - `file_nodes` (or a precisely named equivalent) for folders, files, and
    aliases;
  - shared-permission tables if they are required by the Stage A decision and
    do not already exist in actual migrations; and
  - a resource-scoped permission table for file nodes if the chosen model uses
    one.
- Encode invariants in the database where practical:
  - node kind discriminator (`file`, `folder`, `alias`);
  - `alias_target_id` required only for aliases;
  - `object_key` required only for files;
  - `is_dropbox` legal only for folders;
  - parent/child foreign keys and deletion semantics; and
  - sibling uniqueness under the chosen root strategy.
- Add the indices required for common operations:
  - by `parent_id` for folder listings,
  - by `alias_target_id` for alias maintenance,
  - by principal/resource columns for permission lookups, and
  - by `created_by` if audit and migration paths need it.

Exit criteria:

- both backends can run migrations from scratch;
- the new schema coexists with the old one; and
- Diesel can introspect the new tables cleanly.

### Stage C: update Diesel schema and repository boundaries

- Refresh `src/schema.rs` and introduce new model structs and insertables for
  the file-tree entities.
- Create a focused database module for file-tree operations instead of growing
  legacy flat-file helpers:
  - node creation and lookup;
  - path resolution;
  - alias resolution; and
  - permission lookup helpers.
- Use `diesel-cte-ext` for recursive queries such as:
  - walking ancestors to resolve a path,
  - enumerating descendants for migration/readiness checks, and
  - validating that moves or alias targets do not create cycles when that logic
    is introduced.
- Keep legacy file helpers intact until roadmap 3.1.2 performs the actual data
  migration and 3.2 switches handlers to the new schema.

Exit criteria:

- the codebase has a clean repository API for the new schema;
- recursive hierarchy logic uses `diesel-cte-ext`; and
- no existing runtime path is forced to switch early.

### Stage D: add `rstest` coverage for happy, unhappy, and edge paths

Add or extend parameterized `rstest` coverage for:

- migration shape smoke tests on SQLite and PostgreSQL;
- valid file, folder, and alias insertion paths;
- invalid rows rejected by constraints:
  - alias without target,
  - file without `object_key`,
  - folder marked as alias or drop box illegally,
  - duplicate sibling names,
  - top-level duplicate names under the chosen root strategy;
- recursive query helpers using `diesel-cte-ext`:
  - ancestor path reconstruction,
  - descendant enumeration,
  - alias target lookup; and
- permission lookup and fallback rules for the chosen shared-permission model.

Where a helper takes external state, use fixtures and dependency injection to
keep tests deterministic and backend-agnostic.

### Stage E: add `rstest-bdd` scenarios where behaviour is observable

Create behaviour scenarios only for boundaries that are genuinely observable in
this step. Candidate scenarios:

- a repository can persist a folder tree with files and aliases and return the
  expected hierarchy;
- invalid alias or duplicate-name attempts are rejected with stable errors; and
- permission entries attached to file nodes change visibility decisions at the
  repository or service layer.

Do not force BDD through Hotline transaction handlers in 3.1.1 unless those
handlers are actually switched to the new schema as part of the same change.

Exit criteria:

- there is behaviour coverage for at least one happy, one unhappy, and one edge
  scenario tied to the new schema; and
- the scenarios exercise real repository/service behaviour rather than mock-only
  plumbing.

### Stage F: documentation and operator guidance

- Update `docs/file-sharing-design.md` with the decisions taken in Stage A and
  any implementation-led refinements required by Diesel or backend parity.
- Update `docs/design.md` if the shared permission architecture changes at the
  system level.
- Review `docs/users-guide.md`:
  - if this step changes server behaviour, startup, or administration in a
    user-visible way, document it;
  - if the step is internal-only, record that explicitly during review and do
    not invent user-facing guidance.
- Mark `docs/roadmap.md` item 3.1.1 done only after implementation, tests, and
  documentation are complete.

### Stage G: verification and quality gates

Use `tee` with `set -o pipefail` for every long-running command so failures are
visible even when output is truncated.

Recommended sequence:

1. Prepare PostgreSQL for local tests:

   ```sh
   set -o pipefail
   pg_embedded_setup_unpriv | tee /tmp/pg-setup-3-1-1.log
   ```

2. Verify formatting:

   ```sh
   set -o pipefail
   make check-fmt | tee /tmp/check-fmt-3-1-1.log
   ```

3. Run lint:

   ```sh
   set -o pipefail
   make lint | tee /tmp/lint-3-1-1.log
   ```

4. Run tests:

   ```sh
   set -o pipefail
   make test | tee /tmp/test-3-1-1.log
   ```

5. Lint Markdown:

   ```sh
   set -o pipefail
   make markdownlint | tee /tmp/markdownlint-3-1-1.log
   ```

6. Validate Mermaid diagrams when touched:

   ```sh
   set -o pipefail
   make nixie | tee /tmp/nixie-3-1-1.log
   ```

If a dedicated type-check pass adds signal during implementation, run
`make typecheck` with the same logging pattern before closing the task.
