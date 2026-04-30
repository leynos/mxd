# Align the news schema and migrations (roadmap 4.1.1)

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 4.1.1 requires the repository's news persistence layer to match
`docs/news-schema.md` across both SQLite and PostgreSQL. The target state is
not just "news tables exist"; it is a schema that supports hierarchical
bundles, categories with globally unique identifier (GUID) and sequence
metadata, threaded articles with referential integrity, and normalized
permission tables that later roadmap steps can seed and enforce.

Success is observable when:

- both migration trees produce the schema described in `docs/news-schema.md`
  with the required foreign keys and indices;
- existing news functionality still works against the aligned schema on both
  backends;
- historical news rows are backfilled during upgrade so GUID-bearing schema
  fields are usable immediately after 4.1.1 lands;
- `src/schema.rs`, models, fixtures, and news data-access helpers match the
  migrated schema;
- `rstest` coverage proves happy, unhappy, and edge cases for migration
  structure and schema invariants;
- `rstest-bdd` scenarios cover the user-visible news flows that must keep
  working after the migration;
- local PostgreSQL-backed validation runs via `pg-embed-setup-unpriv`;
- `docs/design.md` records the design decisions taken for the migration
  strategy;
- `docs/users-guide.md` is updated only for user-visible changes, while
  material internal implementation or architectural decisions are recorded in
  `docs/developers-guide.md` or an ADR with a call-out in the developer's guide
  when the rationale is non-trivial;
- `docs/roadmap.md` item 4.1.1 is marked done only after implementation and
  all quality gates pass.

## Constraints

- Preserve deployed-database upgrade safety. Do not rewrite historical
  migration versions in place; add a new migration pair to align the current
  schema with the target shape.
- Keep the two migration trees in lock-step with the same migration version
  number and equivalent semantics.
- Implement the schema in `docs/news-schema.md` as written for:
  `news_bundles`, `news_categories`, `news_articles`, `permissions`, and
  `user_permissions`.
- 4.1.1 includes historical backfill for the newly introduced schema fields,
  especially GUID-bearing rows needed to keep existing news content valid
  immediately after upgrade.
- Preserve existing news routing behaviour for category listing, article
  listing, article fetch, and posting while the storage layer is being
  realigned.
- Use the published `diesel-cte-ext` crate for hierarchical relational logic;
  do not introduce backend-specific recursive query forks outside the existing
  abstraction surface.
- Add `rstest` unit coverage and `rstest-bdd` behavioural coverage where
  applicable, including happy, unhappy, and relevant edge paths.
- Use `pg-embed-setup-unpriv` for local PostgreSQL validation as described in
  `docs/pg-embed-setup-unpriv-users-guide.md`.
- Record design decisions in `docs/design.md`.
- Add explicit indices on every article threading link column as part of 4.1.1
  and verify them with comprehensive unit and behavioural coverage.
- Update `docs/users-guide.md` only for genuine user-visible or
  operator-visible changes.
- Document material internal development-practice, architectural, or domain
  decisions in `docs/developers-guide.md` or an ADR, with a developer-guide
  call-out when the rationale is non-trivial.
- Keep Markdown in en-GB-oxendict and wrap prose at 80 columns.
- Run the applicable repository gates with `tee` and `set -o pipefail` before
  considering the work complete.

## Tolerances (exception triggers)

- Scope: if the work expands beyond approximately 24 files or 900 net lines,
  stop and reassess whether schema alignment, historical backfill, and current
  behaviour preservation are widening into later GUID-addressability or
  permission-enforcement features.
- Migration strategy: if SQLite cannot be aligned without destructive table
  replacement that risks data loss beyond controlled copy-forward, stop and
  document options before proceeding.
- Behaviour: if implementing the schema requires changing protocol-visible news
  behaviour or login privilege semantics now, stop and split that work into the
  correct roadmap item.
- Dependency: if any new crate beyond the already-published
  `diesel-cte-ext` and `pg-embed-setup-unpriv` usage is needed, stop and
  escalate.
- Drift: if the PostgreSQL and SQLite migration semantics cannot be kept
  equivalent, stop and resolve that discrepancy before merging.
- Rework loop: if the required quality gates fail after two targeted fix
  passes, stop and escalate with the captured logs.

## Risks

- Risk: SQLite's `ALTER TABLE` limitations may require table rebuilds for
  `news_categories` and `news_articles`. Severity: high. Likelihood: high.
  Mitigation: use an additive migration that creates replacement tables, copies
  data forward transactionally where possible, and recreates indices and
  constraints explicitly.
- Risk: current fixtures and models assume the old, smaller column set and may
  fail silently or produce partial rows after the schema expands or historical
  rows are backfilled. Severity: high. Likelihood: medium. Mitigation: update
  fixtures, insert helpers, and upgrade tests in the same atomic change as the
  schema update.
- Risk: permission persistence could get accidentally wired into runtime login
  behaviour, conflicting with roadmap item 4.1.3 and the current
  `Privileges::default_user()` fallback. Severity: high. Likelihood: medium.
  Mitigation: create the tables now, but defer catalogue seeding and runtime
  privilege loading to the later roadmap step.
- Risk: existing recursive path lookup SQL may assume uniqueness or nullability
  properties that change under the aligned schema. Severity: medium.
  Likelihood: medium. Mitigation: audit `src/news_path.rs` and add regression
  coverage for root, nested, invalid, and empty-path cases.
- Risk: PostgreSQL and SQLite may drift on defaults or index coverage.
  Severity: medium. Likelihood: medium. Mitigation: add schema-introspection
  tests for both backends rather than relying only on Diesel compile success.

## Progress

- [x] (2026-04-11 00:00Z) Audited roadmap item 4.1.1, current migrations,
      existing news data-access code, and the referenced testing and
      documentation guides.
- [x] (2026-04-11 00:00Z) Drafted this ExecPlan in repository house style.
- [x] (2026-04-13 00:35Z) Confirmed the exact schema delta against
      `docs/news-schema.md`, including the missing permission tables, bundle
      metadata columns, category metadata columns, scoped category uniqueness,
      and the missing article threading indices.
- [x] (2026-04-13 02:35Z) Captured the finalized additive migration strategy
      and backfill rationale in `docs/design.md`.
- [x] (2026-04-13 02:10Z) Added aligned SQLite and PostgreSQL migration pair
      under `00000000000006_align_news_schema`.
- [x] (2026-04-13 02:10Z) Implemented historical backfill for legacy bundle and
      category metadata during upgrade.
- [x] (2026-04-13 02:12Z) Updated Diesel schema, models, and related helpers to
      match the aligned schema, including the permission tables.
- [x] (2026-04-13 02:28Z) Added `rstest`-style unit coverage for schema
      invariants, fresh migration application, and upgrade backfill on both
      backends.
- [x] (2026-04-13 02:18Z) Extended `rstest-bdd` coverage with a nested news
      category routing scenario that runs against migrated databases.
- [x] (2026-04-13 02:33Z) Ran PostgreSQL-backed validation through the
      repository test infrastructure, including the new migration alignment
      tests against an embedded PostgreSQL instance.
- [x] (2026-04-16 00:20Z) Added developer-facing maintenance guidance to
      `docs/developers-guide.md` covering dual-backend news schema alignment,
      SQLite rebuild criteria, and the regression-test hooks for future
      migration work.
- [x] (2026-04-16 00:22Z) Reconfirmed that `docs/users-guide.md` does not need
      a 4.1.1 update because the work intentionally introduced no user-visible
      or operator-visible behaviour change.
- [x] Mark `docs/roadmap.md` item 4.1.1 done after all gates pass.

## Surprises & Discoveries

- The current news schema is already partially implemented, but it is missing
  the normalized permission tables and several columns required by
  `docs/news-schema.md`: `guid`, `created_at`, `add_sn`, and `delete_sn`.
- The existing migration history is split across
  `00000000000001_create_news`, `00000000000002_add_bundles`,
  `00000000000003_add_articles`, and
  `00000000000005_add_bundle_name_parent_index` for both backends.
- Resolved: `src/schema.rs` now defines Diesel tables for `permissions` and
  `user_permissions` alongside `users`, `news_bundles`, `news_categories`,
  `news_articles`, `files`, and `file_acl`.
- `src/news_path.rs` already uses `diesel_cte_ext` for recursive path walking,
  so the hierarchical-query requirement is partly satisfied today and should be
  preserved rather than reinvented.
- Runtime login still grants `Privileges::default_user()` after
  authentication, and the code contains an explicit TODO to load privileges
  from the database later. That confirms 4.1.1 should stop at schema alignment.
- The existing news tests and fixtures are useful regression anchors, but they
  all assume the legacy `NewBundle`, `NewCategory`, and `NewArticle` shapes and
  therefore must be updated in lock-step with the schema.
- The current schema only indexes `news_articles.category_id`, so adding
  explicit threading-link indices will be a real migration change rather than a
  no-op.
- SQLite cannot safely reach the target bundle/category defaults with simple
  `ALTER TABLE` statements because `ADD COLUMN ... DEFAULT CURRENT_TIMESTAMP`
  is not supported for this use case. The SQLite path therefore needs full
  table recreation for `news_bundles`, `news_categories`, and `news_articles`,
  with copy-forward preserving IDs and relationships.
- Backfilling `news_categories.add_sn` from the current article count per
  category, while initializing `delete_sn` to `0`, is the narrowest data repair
  that leaves legacy rows immediately usable without claiming historical
  deletion knowledge the pre-4.1.1 schema never stored.
- The formal repository lint and test Makefile targets now pass in this shell,
  including formatting, Clippy, type checking, Markdown lint, and the full
  `cargo nextest` matrix wrapped by the Makefile gates.

## Decision Log

- Decision: implement 4.1.1 as a new additive migration pair rather than by
  editing old migrations in place. Rationale: preserves upgrade safety for
  existing databases while still allowing 4.1.1 to own the required
  historical-row backfill during upgrade. Date/Author: 2026-04-11 / Codex.
- Decision: create `permissions` and `user_permissions` now, but defer
  catalogue seeding and runtime privilege loading to roadmap item 4.1.3.
  Rationale: keeps 4.1.1 bounded to schema alignment while unblocking the later
  permission work. Date/Author: 2026-04-11 / Codex.
- Decision: backfill historical GUID-bearing fields in 4.1.1 rather than
  leaving legacy rows partially populated until 4.1.2. Rationale: the upgrade
  should leave existing content structurally complete on first boot after the
  schema change. Date/Author: 2026-04-13 / User direction captured by Codex.
- Decision: keep `news_articles` self-referential links restrictive rather
  than cascading on delete. Rationale: threaded deletion semantics belong to
  explicit application logic and later invariants work, not implicit subtree
  deletion side effects. Date/Author: 2026-04-11 / Codex.
- Decision: add explicit indices on `parent_article_id`, `prev_article_id`,
  `next_article_id`, and `first_child_article_id` in 4.1.1. Rationale: the task
  now explicitly requires threading-link index coverage rather than deferring
  to later query-plan tuning. Date/Author: 2026-04-13 / User direction captured
  by Codex.
- Decision: treat 4.1.1 as storage-alignment work with no intentional
  user-visible protocol change. Rationale: browsing, reading, and posting news
  already exist; this step realigns persistence so later GUID and permission
  work has a correct base. Date/Author: 2026-04-11 / Codex.
- Decision: preserve `diesel-cte-ext` as the only recursion abstraction for
  hierarchical news queries. Rationale: it is already the repo standard and is
  explicitly required by the task. Date/Author: 2026-04-11 / Codex.
- Decision: keep user-facing notes in `docs/users-guide.md` strictly limited
  to user-visible change, and document material internal changes in
  `docs/developers-guide.md` or an ADR with a developer-guide call-out when the
  rationale is non-trivial. Rationale: separates operational/user guidance from
  implementation guidance cleanly. Date/Author: 2026-04-13 / User direction
  captured by Codex.
- Decision: rebuild the three SQLite news tables in 4.1.1 instead of mixing
  partial `ALTER TABLE` changes with schema drift. Rationale: it is the only
  practical way to introduce the target defaults and scoped uniqueness while
  preserving legacy rows and stable primary keys. Date/Author: 2026-04-13 /
  Codex.
- Decision: backfill `news_categories.add_sn` from the current per-category
  article count and initialize `delete_sn` to `0`. Rationale: that preserves a
  coherent starting point for later serial-number work without inventing
  historical deletion data. Date/Author: 2026-04-13 / Codex.
- Decision: enforce top-level category-name uniqueness with backend-specific
  indexes because `UNIQUE(name, bundle_id)` alone treats root `NULL`
  `bundle_id` values as distinct. PostgreSQL uses partial unique indexes for
  root rows and scoped non-root rows, while SQLite uses the
  `idx_news_categories_unique` expression index on
  `(name, IFNULL(bundle_id, -1))`. Date/Author: 2026-04-29 / Codex.
- Decision: keep SQLite's legacy `idx_articles_category` index available until
  after `add_sn` backfill has counted articles per category. Rationale: the
  correlated count can otherwise degrade into repeated full scans during
  upgrade. Date/Author: 2026-04-29 / Codex.
- Decision: satisfy the PostgreSQL validation requirement with direct embedded
  PostgreSQL test runs when `make test` is unavailable because `cargo-nextest`
  is missing. Rationale: this preserves behavioural and migration verification
  in the current environment while still recording the blocked formal gate
  explicitly. Date/Author: 2026-04-13 / Codex.

## Outcomes & Retrospective

Intended outcomes once implemented:

- both backends share the same news schema capabilities and referential
  guarantees;
- future roadmap work can rely on GUID, sequence-number, and permission-table
  presence without another schema pivot;
- current news routes remain green after the migration;
- design and user documentation remain aligned with the delivered behaviour.

Retrospective placeholder:

- Implemented:
  - additive migration pair `00000000000006_align_news_schema` for SQLite and
    PostgreSQL;
  - legacy-row backfill for bundle/category GUID and timestamp metadata, plus
    category serial counters;
  - Diesel schema/model alignment including `permissions` and
    `user_permissions`;
  - backend-specific migration verification tests and an added nested-category
    routing BDD scenario.
- Did not implement:
  - runtime privilege loading or permission catalogue seeding.
- Lesson:
  - SQLite's migration limitations are best handled by explicit table rebuilds
    when defaults and scoped uniqueness both change; trying to stage that work
    through incremental `ALTER TABLE` statements would have produced more
    brittle backend divergence.

## Context and orientation

Primary files and modules in current state:

- `docs/roadmap.md`: source of roadmap item 4.1.1 acceptance and downstream
  dependencies 4.1.2 through 4.2.
- `docs/news-schema.md`: target schema for bundles, categories, articles, and
  permissions.
- `docs/developers-guide.md`: developer-facing guide to update if the change
  introduces material implementation or workflow guidance.
- `migrations/sqlite/` and `migrations/postgres/`: current split migration
  trees that must gain a new aligned migration pair.
- `src/schema.rs`: Diesel table definitions that must be regenerated or updated
  to match the new schema.
- `src/models.rs`: current Rust-side row and insert structs for news records.
- `src/news_path.rs`: recursive common table expression (CTE) path lookup
  helper built on `diesel-cte-ext`.
- `src/db/bundles.rs`, `src/db/categories.rs`, `src/db/articles.rs`: news
  persistence helpers that currently assume the old schema.
- `src/login.rs` and `src/privileges.rs`: current privilege handling baseline,
  important because 4.1.1 must not accidentally widen into 4.1.3.
- `test-util/src/fixtures/mod.rs`: seeded database helpers used by the news and
  routing suites.
- `tests/news_categories.rs`, `tests/news_articles.rs`,
  `tests/wireframe_routing_bdd.rs`, and
  `tests/features/wireframe_routing.feature`: current regression anchors for
  news behaviour.
- `docs/design.md`: design record that must capture migration decisions.
- `docs/users-guide.md`: user-facing guide to update only if behaviour or
  operator expectations change.

## Plan of work

### Stage A: lock the migration boundary and schema diff

- Compare `docs/news-schema.md` against the current schema and write a precise
  diff covering:
  - missing tables: `permissions`, `user_permissions`;
  - missing bundle/category/article columns;
  - missing unique constraints and secondary indices;
  - historical-row backfill required during upgrade, especially for GUID fields
    and any new metadata columns.
- Decide the exact migration version to add in both trees, expected to be
  `00000000000006_align_news_schema`.
- Write the migration strategy into `docs/design.md`, especially where SQLite
  requires table recreation rather than simple `ALTER TABLE`.

Exit criteria:

- the migration plan is explicit enough to implement without revisiting schema
  intent;
- the design document records the chosen additive strategy, historical
  backfill approach, and scope boundary versus later permission enforcement.

### Stage B: implement aligned dual-backend migrations

- Add the new SQLite and PostgreSQL migration pair with identical version
  numbers.
- Create `permissions` and `user_permissions` with:
  - primary keys;
  - unique `permissions.code`;
  - foreign keys to `users` and `permissions` with `ON DELETE CASCADE`;
  - indices on `user_permissions.user_id` and
    `user_permissions.permission_id`.
- Align `news_bundles` by ensuring:
  - `parent_bundle_id` self-reference with delete behaviour matching the
    design decision;
  - `guid` and `created_at` columns exist;
  - `UNIQUE(name, parent_bundle_id)` remains enforced;
  - parent and name/parent indices exist.
- Align `news_categories` by ensuring:
  - `bundle_id` foreign key exists;
  - `guid`, `add_sn`, `delete_sn`, and `created_at` columns exist;
  - uniqueness is scoped to `(name, bundle_id)` rather than global `name`;
  - bundle index exists.
- Align `news_articles` by ensuring:
  - all threading link columns exist and remain self-referential;
  - metadata columns match the design document;
  - category foreign key enforces referential integrity;
  - article-category index exists;
  - explicit indices exist on `parent_article_id`, `prev_article_id`,
    `next_article_id`, and `first_child_article_id`.
- Backfill historical bundle/category/article rows during upgrade so newly
  added GUID-bearing and metadata columns are populated immediately after the
  migration completes.
- For SQLite specifically, use copy-forward table recreation when constraint
  changes cannot be expressed safely with `ALTER TABLE`.

Exit criteria:

- a fresh database built from migrations matches the target schema on both
  backends;
- an existing database on the current schema upgrades successfully without
  losing existing rows and with historical backfill applied.

### Stage C: realign Diesel schema and Rust models

- Update `src/schema.rs` to include every new table and column.
- Add `joinable!` and `allow_tables_to_appear_in_same_query!` entries where
  required for the new permission joins.
- Extend or refactor `src/models.rs` so bundle/category/article structs match
  the new columns.
- Add row and insert structs for `Permission` and `UserPermission`, even if
  seeding is deferred.
- Keep helper construction readable: introduce targeted constructors or helper
  structs instead of pushing long parameter lists through fixtures.

Exit criteria:

- Rust compiles against the aligned schema on both feature sets;
- fixtures and data-access helpers can insert valid rows without ad-hoc null
  juggling.

### Stage D: update hierarchical helpers, fixtures, and current news paths

- Audit `src/news_path.rs`, `src/db/bundles.rs`, `src/db/categories.rs`, and
  `src/db/articles.rs` against the new uniqueness/nullability rules.
- Confirm the current recursive CTE path lookup remains correct for:
  - root lookups;
  - nested bundle lookups;
  - category resolution under a bundle;
  - invalid paths.
- Update `test-util` news fixtures so they populate any newly required data and
  keep current routing tests readable.
- Keep current article insertion behaviour stable while the richer schema is
  introduced, even though stored rows are now backfilled with GUID-bearing
  fields.

Exit criteria:

- current news helper APIs remain usable;
- existing routing and integration tests do not need protocol-visible changes
  to keep working.

### Stage E: add verification coverage

Add `rstest` unit coverage for at least:

- fresh migration application on SQLite and PostgreSQL;
- upgrade from the pre-4.1.1 schema to the aligned schema on both backends;
- presence of expected foreign keys and indices, using backend-appropriate
  schema introspection;
- correctness of historical backfill for existing bundle/category/article rows;
- uniqueness and referential-integrity unhappy paths;
- edge cases such as root-level categories (`bundle_id IS NULL`) and threaded
  articles with nullable link fields.

Add or extend `rstest-bdd` behavioural coverage where applicable:

- happy: browsing root and nested news structures still works after the schema
  change;
- happy: threaded article navigation still works with the newly indexed link
  columns in place;
- unhappy: invalid news path still returns the existing protocol error;
- edge: empty or mixed bundle/category roots still behave as before;
- edge: historical data upgraded through the new migration still supports the
  existing browse/read/post flows.

Reuse existing binary-backed news and routing scenarios where that gives enough
coverage; add a dedicated feature file only if the current scenarios cannot
express the migration-sensitive behaviour clearly.

Exit criteria:

- unit tests prove the storage contract directly;
- behavioural tests prove the schema change and backfill did not regress
  observable news flows.

### Stage F: documentation, roadmap close-out, and validation

- Update `docs/design.md` with the migration approach, permission-table scope,
  and any backend-specific rationale.
- Update `docs/developers-guide.md` if the implementation introduces material
  internal guidance about migrations, schema maintenance, or testing practice.
- If the rationale for those internal choices is non-trivial, write or update
  an ADR and add a call-out in `docs/developers-guide.md`.
- Update `docs/users-guide.md` only if there is a genuine behaviour or
  operator-facing change.
- Mark roadmap item 4.1.1 done in `docs/roadmap.md` only after every gate
  passes and the change is ready to land.

## Concrete implementation checklist

1. Add the new migration version under both `migrations/sqlite/` and
   `migrations/postgres/`.
2. Create `permissions` and `user_permissions` in both backends.
3. Add the missing bundle/category/article columns and indices.
4. Rebuild SQLite tables where constraint changes require it.
5. Preserve and backfill existing news rows during upgrade.
6. Update `src/schema.rs`.
7. Update `src/models.rs` and any helper constructors.
8. Update news fixtures and data-access helpers.
9. Add threading-link indices on all four article link columns.
10. Add `rstest` schema, backfill, and migration coverage.
11. Extend `rstest-bdd` news regression coverage where applicable.
12. Update `docs/design.md`.
13. Update `docs/developers-guide.md` or an ADR if needed.
14. Update `docs/users-guide.md` only if needed.
15. Mark `docs/roadmap.md` item 4.1.1 done only after validation succeeds.

## Verification and quality gates

Run local PostgreSQL setup first, then the relevant gates. Capture output with
`tee` and preserve exit status with `set -o pipefail`.

Recommended command pattern:

```sh
set -o pipefail
PROJECT=$(basename "$PWD")
BRANCH=$(git branch --show-current | tr '/ ' '__')
```

1. Prepare the embedded PostgreSQL runtime:

   ```sh
   pg-embed-setup-unpriv \
     | tee /tmp/pg-setup-$PROJECT-$BRANCH.log
   ```

2. Format Markdown and Rust sources after the schema and documentation edits:

   ```sh
   make fmt | tee /tmp/fmt-$PROJECT-$BRANCH.log
   ```

3. Verify Rust formatting:

   ```sh
   make check-fmt | tee /tmp/check-fmt-$PROJECT-$BRANCH.log
   ```

   Equivalent direct Rust gate:

   ```sh
   cargo fmt --workspace -- --check
   ```

4. Run lint checks:

   ```sh
   make lint | tee /tmp/lint-$PROJECT-$BRANCH.log
   ```

   The Rust lint matrix must cover each mutually exclusive feature set with
   test support enabled and warnings denied:

   ```sh
   cargo clippy --no-default-features \
     --features "postgres test-support legacy-networking" \
     --workspace --all-targets -- -D warnings
   cargo clippy --features "sqlite test-support" \
     --workspace --all-targets -- -D warnings
   cargo clippy --no-default-features --features "sqlite toml test-support" \
     --workspace --all-targets -- -D warnings
   ```

5. Run all tests:

   ```sh
   make test | tee /tmp/test-$PROJECT-$BRANCH.log
   ```

   The Rust test matrix must run `cargo nextest` for each feature set:

   ```sh
   RUSTFLAGS="-D warnings" cargo nextest run --no-default-features \
     --features "postgres test-support legacy-networking" \
     --workspace --all-targets
   RUSTFLAGS="-D warnings" cargo nextest run --features "sqlite test-support" \
     --workspace --all-targets
   RUSTFLAGS="-D warnings" cargo nextest run --no-default-features \
     --features "sqlite toml test-support" \
     --workspace --all-targets
   ```

6. Run type checks:

   ```sh
   make typecheck | tee /tmp/typecheck-$PROJECT-$BRANCH.log
   ```

7. Validate Markdown:

   ```sh
   make markdownlint | tee /tmp/markdownlint-$PROJECT-$BRANCH.log
   ```

8. Validate Mermaid diagrams if any diagram-bearing Markdown changed:

   ```sh
   make nixie | tee /tmp/nixie-$PROJECT-$BRANCH.log
   ```
