# Align the news schema and migrations (roadmap 4.1.1)

This ExecPlan is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as
work proceeds.

Status: DRAFT

PLANS.md does not exist in this repository.

## Purpose / big picture

Roadmap item 4.1.1 requires the repository's news persistence layer to match
`docs/news-schema.md` across both SQLite and PostgreSQL. The target state is
not just "news tables exist"; it is a schema that supports hierarchical
bundles, categories with GUID and sequence metadata, threaded articles with
referential integrity, and normalized permission tables that later roadmap
steps can seed and enforce.

Success is observable when:

- both migration trees produce the schema described in `docs/news-schema.md`
  with the required foreign keys and indices;
- existing news functionality still works against the aligned schema on both
  backends;
- `src/schema.rs`, models, fixtures, and news data-access helpers match the
  migrated schema;
- `rstest` coverage proves happy, unhappy, and edge cases for migration
  structure and schema invariants;
- `rstest-bdd` scenarios cover the user-visible news flows that must keep
  working after the migration;
- local PostgreSQL-backed validation runs via `pg-embed-setup-unpriv`;
- `docs/design.md` records the design decisions taken for the migration
  strategy;
- `docs/users-guide.md` is updated if any server behaviour or operator-facing
  expectations change;
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
- Update `docs/users-guide.md` only for genuine user-visible or
  operator-visible changes; if behaviour is unchanged, record that explicitly
  in the implementation notes and keep the guide unchanged.
- Keep Markdown in en-GB-oxendict and wrap prose at 80 columns.
- Run the applicable repository gates with `tee` and `set -o pipefail` before
  considering the work complete.

## Tolerances (exception triggers)

- Scope: if the work expands beyond approximately 24 files or 900 net lines,
  stop and reassess whether schema alignment is being conflated with roadmap
  items 4.1.2 or 4.1.3.
- Migration strategy: if SQLite cannot be aligned without destructive table
  replacement that risks data loss beyond controlled copy-forward, stop and
  document options before proceeding.
- Behaviour: if implementing the schema requires changing protocol-visible news
  behaviour or login privilege semantics now, stop and split that work into
  the correct roadmap item.
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
  Mitigation: use an additive migration that creates replacement tables,
  copies data forward transactionally where possible, and recreates indices and
  constraints explicitly.
- Risk: current fixtures and models assume the old, smaller column set and may
  fail silently or produce partial rows after the schema expands. Severity:
  high. Likelihood: medium. Mitigation: update fixtures and insert helpers in
  the same atomic change as the schema update.
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
- [ ] Finalize migration strategy and capture it in `docs/design.md`.
- [ ] Add aligned SQLite and PostgreSQL migration pair.
- [ ] Update Diesel schema, models, and news fixtures/helpers.
- [ ] Add `rstest` coverage for schema invariants and migration behaviour.
- [ ] Add or extend `rstest-bdd` coverage for unaffected news behaviour.
- [ ] Run PostgreSQL-backed validation via `pg-embed-setup-unpriv`.
- [ ] Update `docs/users-guide.md` if behaviour changes.
- [ ] Mark `docs/roadmap.md` item 4.1.1 done after all gates pass.

## Surprises & Discoveries

- The current news schema is already partially implemented, but it is missing
  the normalized permission tables and several columns required by
  `docs/news-schema.md`:
  `guid`, `created_at`, `add_sn`, and `delete_sn`.
- The existing migration history is split across
  `00000000000001_create_news`, `00000000000002_add_bundles`,
  `00000000000003_add_articles`, and
  `00000000000005_add_bundle_name_parent_index` for both backends.
- `src/schema.rs` currently exposes only `users`, `news_bundles`,
  `news_categories`, `news_articles`, `files`, and `file_acl`; there are no
  Diesel definitions yet for `permissions` or `user_permissions`.
- `src/news_path.rs` already uses `diesel_cte_ext` for recursive path walking,
  so the hierarchical-query requirement is partly satisfied today and should be
  preserved rather than reinvented.
- Runtime login still grants `Privileges::default_user()` after
  authentication, and the code contains an explicit TODO to load privileges
  from the database later. That confirms 4.1.1 should stop at schema
  alignment.
- The existing news tests and fixtures are useful regression anchors, but they
  all assume the legacy `NewBundle`, `NewCategory`, and `NewArticle` shapes and
  therefore must be updated in lock-step with the schema.

## Decision Log

- Decision: implement 4.1.1 as a new additive migration pair rather than by
  editing old migrations in place. Rationale: preserves upgrade safety for
  existing databases and gives roadmap item 4.1.2 a clean boundary for data
  migration/backfill. Date/Author: 2026-04-11 / Codex.
- Decision: create `permissions` and `user_permissions` now, but defer
  catalogue seeding and runtime privilege loading to roadmap item 4.1.3.
  Rationale: keeps 4.1.1 bounded to schema alignment while unblocking the
  later permission work. Date/Author: 2026-04-11 / Codex.
- Decision: keep `news_articles` self-referential links restrictive rather
  than cascading on delete. Rationale: threaded deletion semantics belong to
  explicit application logic and later invariants work, not implicit subtree
  deletion side effects. Date/Author: 2026-04-11 / Codex.
- Decision: treat 4.1.1 as storage-alignment work with no intentional
  user-visible protocol change. Rationale: browsing, reading, and posting news
  already exist; this step realigns persistence so later GUID and permission
  work has a correct base. Date/Author: 2026-04-11 / Codex.
- Decision: preserve `diesel-cte-ext` as the only recursion abstraction for
  hierarchical news queries. Rationale: it is already the repo standard and is
  explicitly required by the task. Date/Author: 2026-04-11 / Codex.

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
- Did not implement:
- Lesson:

## Context and orientation

Primary files and modules in current state:

- `docs/roadmap.md`: source of roadmap item 4.1.1 acceptance and downstream
  dependencies 4.1.2 through 4.2.
- `docs/news-schema.md`: target schema for bundles, categories, articles, and
  permissions.
- `migrations/sqlite/` and `migrations/postgres/`: current split migration
  trees that must gain a new aligned migration pair.
- `src/schema.rs`: Diesel table definitions that must be regenerated or updated
  to match the new schema.
- `src/models.rs`: current Rust-side row and insert structs for news records.
- `src/news_path.rs`: recursive CTE path lookup helper built on
  `diesel-cte-ext`.
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
  - any nullability/default changes needed to preserve existing data.
- Decide the exact migration version to add in both trees, expected to be
  `00000000000006_align_news_schema`.
- Write the migration strategy into `docs/design.md`, especially where SQLite
  requires table recreation rather than simple `ALTER TABLE`.

Exit criteria:

- the migration plan is explicit enough to implement without revisiting schema
  intent;
- the design document records the chosen additive strategy and scope boundary
  versus 4.1.2/4.1.3.

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
  - article-category index exists.
- For SQLite specifically, use copy-forward table recreation when constraint
  changes cannot be expressed safely with `ALTER TABLE`.

Exit criteria:

- a fresh database built from migrations matches the target schema on both
  backends;
- an existing database on the current schema upgrades successfully without
  losing existing rows.

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
  introduced; do not widen into GUID-based addressing yet.

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
- uniqueness and referential-integrity unhappy paths;
- edge cases such as root-level categories (`bundle_id IS NULL`) and threaded
  articles with nullable link fields.

Add or extend `rstest-bdd` behavioural coverage where applicable:

- happy: browsing root and nested news structures still works after the schema
  change;
- unhappy: invalid news path still returns the existing protocol error;
- edge: empty or mixed bundle/category roots still behave as before.

Reuse existing binary-backed news and routing scenarios where that gives enough
coverage; add a dedicated feature file only if the current scenarios cannot
express the migration-sensitive behaviour clearly.

Exit criteria:

- unit tests prove the storage contract directly;
- behavioural tests prove the schema change did not regress observable news
  flows.

### Stage F: documentation, roadmap close-out, and validation

- Update `docs/design.md` with the migration approach, permission-table scope,
  and any backend-specific rationale.
- Update `docs/users-guide.md` only if there is a genuine behaviour or
  operator-facing change. If behaviour remains unchanged, record that decision
  in the change notes and leave the guide untouched.
- Mark roadmap item 4.1.1 done in `docs/roadmap.md` only after every gate
  passes and the change is ready to land.

## Concrete implementation checklist

1. Add the new migration version under both `migrations/sqlite/` and
   `migrations/postgres/`.
2. Create `permissions` and `user_permissions` in both backends.
3. Add the missing bundle/category/article columns and indices.
4. Rebuild SQLite tables where constraint changes require it.
5. Preserve or copy forward existing news rows during upgrade.
6. Update `src/schema.rs`.
7. Update `src/models.rs` and any helper constructors.
8. Update news fixtures and data-access helpers.
9. Add `rstest` schema/migration coverage.
10. Extend `rstest-bdd` news regression coverage where applicable.
11. Update `docs/design.md`.
12. Update `docs/users-guide.md` if needed.
13. Mark `docs/roadmap.md` item 4.1.1 done only after validation succeeds.

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
   pg_embedded_setup_unpriv \
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

4. Run lint checks:

   ```sh
   make lint | tee /tmp/lint-$PROJECT-$BRANCH.log
   ```

5. Run all tests:

   ```sh
   make test | tee /tmp/test-$PROJECT-$BRANCH.log
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

## Open questions to resolve during implementation

- Should `guid` remain nullable in the aligned schema until 4.1.2 backfills
  historical data, or should 4.1.1 generate placeholder GUIDs during upgrade?
  Default position: keep the schema permissive enough that 4.1.2 owns the
  historical backfill.
- Do we need explicit indices on the article threading link columns now, or is
  `category_id` the only index required by 4.1.1 acceptance? Default position:
  implement the indices explicitly required by `docs/news-schema.md` first,
  then add more only if tests or query plans justify them.
- Should any behaviour-level user guide note be added if the change is purely
  internal? Default position: no user-guide change unless operator-visible
  setup or behaviour actually changes.
