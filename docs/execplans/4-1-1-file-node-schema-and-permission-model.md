# Task 4.1.1: Align the news schema and migrations

This Execution Plan (ExecPlan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises &
Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up
to date as work proceeds.

Status: NOT STARTED

This document must be maintained in accordance with the execplans workflow,
even though the `execplans` skill is not available in this session.

## Purpose / big picture

Roadmap task 4.1.1 requires MXD's database layer to match
`docs/news-schema.md` for both SQLite and PostgreSQL. After this change, a
fresh database created through embedded migrations will contain the complete
news structure: permission catalogue tables, user-to-permission links, nested
news bundles, bundle-scoped categories, and threaded news articles with the
metadata required by the design.

Observable success:

- Running the embedded migrations on SQLite and PostgreSQL produces the same
  logical schema.
- Referential integrity is enforced for all new foreign keys.
- Required indices exist for the news traversal and permission lookup paths.
- Diesel's schema and models compile for both backend feature sets.
- Automated tests prove happy paths, unhappy paths, and backend parity.

## Constraints

- The implementation must satisfy roadmap item 4.1.1 only. It may create the
  permission tables now, but it must not silently absorb roadmap items 4.1.2
  or 4.1.3.
- SQLite and PostgreSQL migration version numbers must stay aligned.
- The implementation must preserve upgradeability for existing databases that
  already ran migrations `00000000000000` through `00000000000005`.
- Tests must use `rstest` for unit-style coverage and `rstest-bdd` where the
  schema change creates observable behaviour worth expressing in Gherkin.
- Local PostgreSQL validation must use the published
  `pg-embed-setup-unpriv` crate, via the existing `test-util` helpers where
  possible, rather than ad hoc ignored tests.
- Hierarchical relational logic must continue to use the published
  `diesel-cte-ext` crate where recursive traversal helpers are touched.
- All affected Rust modules must keep module-level `//!` comments and remain
  under the repository's 400-line file limit.
- Documentation updates are required in `docs/design.md`, `docs/users-guide.md`,
  and `docs/roadmap.md` once the feature is complete.
- Quality gates remain mandatory: `make check-fmt`, `make lint`, `make test`,
  and the relevant Markdown checks.

## Tolerances (exception triggers)

- **Schema scope**: If matching `docs/news-schema.md` requires inventing new
  tables or columns not described there or in `docs/design.md`, stop and
  resolve the design gap before coding.
- **Migration history**: If the only workable approach is to rewrite or delete
  previously shipped migration directories, stop and escalate.
- **Permission semantics**: If task 4.1.1 cannot cleanly stop short of loading
  permissions into runtime sessions, stop and split the behavioural work into
  task 4.1.3 instead of folding it in.
- **Testing**: If PostgreSQL tests cannot be made deterministic with the
  published embedded-cluster path, stop after recording the blocker and the
  failing command output.
- **Blast radius**: If the schema change forces unrelated wireframe routing or
  login compatibility refactors, stop and separate those changes.

## Risks

- Risk: SQLite cannot express the required schema evolution with simple
  `ALTER TABLE` statements.
  Severity: high. Likelihood: high.
  Mitigation: plan for table rebuild migrations that copy data into replacement
  tables, preserve IDs, recreate indices, and validate row counts.

- Risk: current code assumes globally unique category names, but the target
  schema makes category names bundle-scoped.
  Severity: high. Likelihood: medium.
  Mitigation: audit path resolution, creation helpers, and tests for hidden
  global-uniqueness assumptions before finalizing the migration shape.

- Risk: self-referential article foreign keys can block deletes or create
  inconsistent copy order during SQLite table rebuilds.
  Severity: medium. Likelihood: medium.
  Mitigation: copy articles in ID order, preserve IDs explicitly, and add
  unhappy-path tests around invalid linkage.

- Risk: permission tables overlap conceptually with the current in-memory
  session privilege bitmap.
  Severity: medium. Likelihood: medium.
  Mitigation: document clearly that 4.1.1 creates persistence structures only;
  runtime loading and enforcement remain a follow-on in 4.1.3.

## Progress

- [ ] Stage A: Crosswalk the target schema against the current implementation.
- [ ] Stage B: Write backend-specific migration pair
  `00000000000006_align_news_schema`.
- [ ] Stage C: Update Diesel schema, models, and DB helpers for the new shape.
- [ ] Stage D: Add SQLite and PostgreSQL schema-verification tests with
  `rstest`.
- [ ] Stage E: Add behavioural coverage with `rstest-bdd` where the new schema
  changes observable behaviour.
- [ ] Stage F: Update `docs/design.md` and `docs/users-guide.md`.
- [ ] Stage G: Run quality gates and mark roadmap item 4.1.1 done.

## Surprises & discoveries

- The current embedded schema is only a subset of the target design:
  `news_bundles` lacks `guid` and `created_at`; `news_categories` lacks `guid`,
  `add_sn`, `delete_sn`, and `created_at`; there are no `permissions` or
  `user_permissions` tables at all.
- `news_categories.name` is currently globally unique because the earliest
  migration created `news_categories` with `name TEXT NOT NULL UNIQUE`. That
  conflicts with the target model, which scopes uniqueness by `bundle_id`.
- The repository already depends on `diesel-cte-ext` and
  `pg-embed-setup-unpriv`, so this task should integrate those existing
  dependencies rather than add bespoke alternatives.
- The roadmap explicitly separates structure creation (4.1.1) from permission
  seeding and runtime honouring (4.1.3). The plan should respect that split.

## Decision log

- Decision: add a new forward migration pair instead of rewriting historical
  migration directories.
  Rationale: existing databases, embedded migrations, and template-hash-based
  PostgreSQL test helpers all assume append-only migration history.
  Date/Author: Plan phase.

- Decision: create `permissions` and `user_permissions` tables in 4.1.1 but
  defer seeding the full 38 permission rows to 4.1.3.
  Rationale: the roadmap makes schema alignment and catalogue seeding distinct
  deliverables; keeping that split reduces behavioural risk.
  Date/Author: Plan phase.

- Decision: keep current session privilege bitflags as the runtime authority
  during 4.1.1.
  Rationale: loading permissions from the new tables is an application
  behaviour change and belongs to 4.1.3, not this migration task.
  Date/Author: Plan phase.

- Decision: favour explicit schema-introspection tests for indices, foreign
  keys, and uniqueness rules.
  Rationale: acceptance is about DDL correctness, not just whether higher-level
  handlers still work.
  Date/Author: Plan phase.

## Outcomes & retrospective

(To be populated at completion.)

## Context and orientation

### Current repository state

- `migrations/sqlite/00000000000001_create_news/up.sql`,
  `00000000000002_add_bundles/up.sql`, and
  `00000000000003_add_articles/up.sql` build the current news schema in three
  incremental steps. The PostgreSQL tree mirrors that split.
- `src/schema.rs` and `src/models.rs` currently model only:
  - `news_bundles(id, parent_bundle_id, name)`
  - `news_categories(id, name, bundle_id)`
  - `news_articles(id, category_id, parent_article_id, prev_article_id,
    next_article_id, first_child_article_id, title, poster, posted_at, flags,
    data_flavor, data)`
- `src/db/bundles.rs`, `src/db/categories.rs`, `src/db/articles.rs`, and
  `src/news_path.rs` already implement hierarchical lookup and insertion logic.
- `tests/news_categories.rs` and `tests/news_articles.rs` exercise the current
  wireframe-visible news flows.

### Target schema deltas from `docs/news-schema.md`

The target design introduces or tightens the following elements:

- `permissions` and `user_permissions` tables, with scope-aware permission
  metadata and many-to-many links to `users`.
- `news_bundles.guid` and `news_bundles.created_at`.
- `news_categories.guid`, `news_categories.add_sn`,
  `news_categories.delete_sn`, and `news_categories.created_at`.
- Bundle-scoped category uniqueness instead of global category-name
  uniqueness.
- Explicit indices that support:
  - permission lookup by user and permission,
  - bundle traversal by `parent_bundle_id`,
  - category traversal by `bundle_id`,
  - article lookup by `category_id`,
  - any additional lookup paths introduced by GUIDs or threading constraints.

### Out of scope for this task

- Migrating historic rows into GUID-populated structures beyond what the schema
  migration itself must do for existing tables.
- Seeding the 38 Hotline permission definitions.
- Loading permission rows into sessions or enforcing them in news handlers.
- Writing the TLA+ threading specification from roadmap item 4.1.4.

## Plan of work

### Stage A: Freeze the exact target shape

1. Crosswalk `docs/news-schema.md`, `docs/design.md`, and the current migration
   trees into a concrete table-by-table diff.
2. Record the final column list, nullability, unique constraints, foreign-key
   delete behaviour, and index list in `docs/design.md` so implementation is
   not driven by unstated assumptions.
3. Confirm which parts are intentionally deferred:
   - no permission seed data yet,
   - no runtime permission loading yet,
   - no news-content backfill beyond preserving existing rows while reshaping
     tables.

### Stage B: Add the new migrations

Create `migrations/sqlite/00000000000006_align_news_schema/` and
`migrations/postgres/00000000000006_align_news_schema/`.

For PostgreSQL:

- Add the missing columns to `news_bundles` and `news_categories`.
- Relax the old global uniqueness rule on `news_categories.name` and replace
  it with the correct bundle-scoped uniqueness.
- Create `permissions` and `user_permissions`.
- Add missing indices and unique constraints.
- Preserve existing `news_articles` rows and linkage data unchanged unless a
  constraint requires explicit cleanup first.

For SQLite:

- Rebuild `news_categories` because SQLite cannot safely replace the current
  global `UNIQUE(name)` constraint in place.
- Rebuild any other table whose new columns or constraints cannot be expressed
  safely with `ALTER TABLE`.
- Copy data into replacement tables with IDs preserved.
- Recreate all foreign keys and indices before dropping the old tables.

The migration must be reversible enough to satisfy Diesel's `down.sql`
expectations, but the primary focus is correct forward migration for existing
installations.

### Stage C: Align Diesel's Rust surface

1. Update `src/schema.rs` to reflect the final table shape.
2. Extend `src/models.rs` with:
   - new fields on `Bundle`, `Category`, and insertable structs,
   - new `Permission`, `NewPermission`, `UserPermission`, and
     `NewUserPermission` models if the migration tests need typed access.
3. Audit the DB helpers for assumptions broken by the new schema:
   - `src/db/bundles.rs`
   - `src/db/categories.rs`
   - `src/db/articles.rs`
   - `src/news_path.rs`
4. Keep hierarchical traversal on the existing `diesel-cte-ext` path where
   recursive logic is touched; do not replace it with bespoke one-off SQL.

### Stage D: Add schema-verification tests with `rstest`

Add backend-aware tests that validate the DDL itself, not just handler-level
success.

SQLite coverage:

- migrate an in-memory database and verify the expected tables, columns,
  foreign keys, unique constraints, and indices via `PRAGMA table_info`,
  `PRAGMA foreign_key_list`, and `PRAGMA index_list`.
- verify happy paths:
  - inserting sibling categories with the same name under different bundles
    succeeds,
  - inserting permission rows and user links succeeds,
  - deleting a user cascades through `user_permissions`.
- verify unhappy paths:
  - duplicate category name within the same bundle is rejected,
  - dangling foreign keys are rejected,
  - duplicate `(user_id, permission_id)` links are rejected.

PostgreSQL coverage:

- use `pg-embed-setup-unpriv` through `test-util::PostgresTestDb` or the
  equivalent shared-cluster helper.
- introspect schema using `information_schema`, `pg_indexes`, and catalog
  queries so the same invariants are verified against the PostgreSQL tree.
- remove or supersede the current ignored PostgreSQL audit smoke test if the
  new coverage makes it redundant.

Prefer parameterized `rstest` cases so SQLite and PostgreSQL can share the
same logical assertions where practical.

### Stage E: Add behavioural coverage with `rstest-bdd`

Pure DDL is not user-visible, so most validation belongs in `rstest` tests.
BDD still applies where the new schema changes observable server behaviour.

Candidate behavioural coverage:

- a scenario proving that two bundles may each contain a category with the same
  name and clients can still traverse them by full path correctly;
- a scenario proving that invalid nested paths remain rejected after the schema
  change;
- if no user-visible behaviour changes remain after implementation, keep BDD
  minimal and document why the remaining acceptance criteria are internal.

The BDD suite should cover at least one happy path and one unhappy path tied to
the schema realignment, not generic news browsing already covered elsewhere.

### Stage F: Update the documentation set

1. Update `docs/design.md` with:
   - the chosen migration strategy,
   - the final constraint and index matrix,
   - the decision to defer permission seeding and session loading.
2. Update `docs/users-guide.md` if administrators or operators need to know
   about any migration-time or behaviour-visible change. If there is no
   user-visible change, say so explicitly rather than leaving the guide silent.
3. If implementation-level clarifications differ from `docs/news-schema.md`,
   update that document too so the schema reference and actual migrations stay
   aligned.
4. When all work is complete, mark roadmap item 4.1.1 as done in
   `docs/roadmap.md`.

### Stage G: Run quality gates

Because the environment truncates long command output, run the gateways
through `tee` with `set -o pipefail`.

Suggested command set:

```sh
set -o pipefail && make fmt | tee /tmp/4-1-1-make-fmt.log
set -o pipefail && make check-fmt | tee /tmp/4-1-1-make-check-fmt.log
set -o pipefail && make lint | tee /tmp/4-1-1-make-lint.log
set -o pipefail && make test | tee /tmp/4-1-1-make-test.log
set -o pipefail && make markdownlint | tee /tmp/4-1-1-make-markdownlint.log
```

If any Mermaid diagrams change, also run:

```sh
set -o pipefail && make nixie | tee /tmp/4-1-1-make-nixie.log
```

Review the logs after each run so truncated terminal output does not hide the
real failure.
