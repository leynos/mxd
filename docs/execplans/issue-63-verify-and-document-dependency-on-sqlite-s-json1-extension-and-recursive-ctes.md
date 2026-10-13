# Verify and document the SQLite JSON1 and recursive-CTE dependency

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`, `Decision log`,
`Outcomes & retrospective`, `Conformance basis`, and `Verification plan` must
be kept up to date as work proceeds.

Status: IN PROGRESS

## Purpose / big picture

mxd stores its users and file hierarchy in SQLite. Two SQLite capabilities are
load-bearing: the JSON1 extension (the `json_each` table-valued function, used
to split slash-delimited paths into segments) and recursive common table
expressions (CTEs, used to walk the file-node hierarchy). A SQLite library
built without `json_each`, or older than 3.35.0, would let the daemon start and
then fail at the first path lookup — a runtime failure in front of users rather
than a startup failure in front of an operator.

After this change an operator who starts any of the three entry points (the
legacy server, the Wireframe server, and the `create-user` administrative
command) against an incompatible SQLite library sees the process refuse to
start and exit with an error naming the missing capability, before the
listening socket is bound and before any row is written. The error text names
the capability and the version floor, so the operator knows which package to
fix. `README.md` and the onboarding guides state the requirement in terms the
operator can check from a shell.

The observable demonstration is: run

```sh
make sqlite && target/debug/mxd --database mxd.db --bind 127.0.0.1:5500
```

against a compatible SQLite library and see the server bind normally; run the
same command against a library lacking `json_each` and see a non-zero exit with
a message containing `SQLite JSON1 json_each probe failed`.

## Constraints

Hard invariants that must hold throughout implementation. Violation requires
escalation, not a workaround.

- `audit_sqlite_features` keeps the signature
  `async fn audit_sqlite_features(conn: &mut DbConnection) -> QueryResult<()>`.
  It already exists; this work extends it and does not introduce a second audit
  mechanism.
- The PostgreSQL audit (`audit_postgres_features`) and all PostgreSQL behaviour
  are out of scope and must not change.
- `establish_pool`, the migration connection handling, and the SQLite gating in
  `src/server/legacy/mod.rs` are unchanged.
- The audit must run before the listening socket is bound and before any data
  is written, at all three entry points.
- No new external dependency may be added.
- Documentation must not claim that SQLite is bundled with the project, and must
  not describe release-packaging dependencies. Confirmed: `Cargo.lock` resolves
  `libsqlite3-sys 0.35.0` with only the `pkg-config` and `vcpkg` build
  dependencies, i.e. the system library is linked, not vendored.
- Documentation and comments use en-GB-oxendict spelling.
- No single code file may exceed 400 lines (repository rule in `AGENTS.md`).
  `src/db/audit.rs` is currently about 90 lines, so the added test module must
  stay within roughly 300 further lines. If it grows past that, move the test
  module to a sibling file rather than breaching the limit.

## Tolerances (exception triggers)

Thresholds that trigger escalation when breached.

- Scope: if implementation requires changes to more than 8 tracked files or more
  than 400 net lines, stop and escalate.
- Interface: if any public API signature other than adding `#[cfg(test)]`
  internals must change, stop and escalate.
- Dependencies: if a new external dependency appears necessary, stop and
  escalate.
- Iterations: if a single gate still fails after three fix attempts, stop and
  escalate with the captured log.
- Ambiguity: if the required minimum SQLite version proves to conflict with what
  the build actually needs, stop and present the evidence rather than silently
  changing the floor.

## Risks

- Risk: the `json_each` `key` and `value` columns may not map cleanly onto
  Diesel's `i32`/`String` `FromSql` implementations for the SQLite backend,
  because SQLite is dynamically typed and `json_each` declares no column types.
  Severity: medium Likelihood: medium Mitigation: probe the exact query shape
  with the `sqlite3` CLI before writing the Rust code (done), and if the typed
  load fails, fall back to loading `CAST(key AS INTEGER)` and
  `CAST(value AS TEXT)` so the storage classes are explicit.
- Risk: `pool.get()` in the Wireframe path returns a `PoolError` rather than a
  `diesel::result::Error`, so the `.context(...)` chain uses `anyhow` rather
  than `diesel::result::QueryResult`. Severity: low Likelihood: high (this is
  certain, not a risk — recorded here so the implementer does not try to reuse
  the `QueryResult` shape). Mitigation: return `anyhow::Result` at that call
  site, as the surrounding code already does.
- Risk: adding an audit call to `create-user` changes the behaviour of a
  command that an operator may run against an existing database with an
  incomplete schema. Severity: low Likelihood: low Mitigation: the audit is
  placed after connection establishment and before `apply_migrations`, so it
  runs on every path that already ran migrations first; there is no new failure
  mode for a compatible library.
- Risk: the 3.35.0 floor is derived from Diesel's
  `returning_clauses_for_sqlite_3_35` feature, not from the JSON1 requirement.
  Documenting it as a JSON1 requirement would be wrong. Severity: medium
  Likelihood: medium Mitigation: state the two requirements separately
  everywhere (see `Plan of work`, stage D).

## Progress

- [x] (2026-10-13) Reconnaissance: read `src/db/audit.rs`, the three entry
  points, the typed raw-SQL pattern in `src/db/files.rs`, the four
  documentation targets, `Cargo.toml` features, and `clippy.toml`.
- [x] (2026-10-13) Confirmed with the `sqlite3` CLI that the intended probe
  queries return the expected shapes, and confirmed the JSON1 provenance facts
  that the documentation claims must rest on.
- [x] (2026-10-13) Renamed the branch and set the Lody session title.
- [x] (2026-10-13) Wrote this ExecPlan.
- [x] (2026-10-13) EP-M1 Red: added the `#[cfg(test)]` validator tests with
  stub validators. 13 rejecting cases failed, 4 accepting cases passed,
  confirming the tests discriminate. Transcript at
  `/tmp/test-audit-issue-63-red.out`.
- [x] (2026-10-13) EP-M1 Green: implemented the three probes and the three
  validators. 22 validator cases pass (`/tmp/test-audit-issue-63-green.out`),
  and the real-connection `test_audit_features` passes
  (`/tmp/test-audit-e2e-issue-63.out`), which discharges V5 against the real
  library.
- [x] (2026-10-13) EP-M1 committed as `00d023a`.
- [x] (2026-10-13) EP-M2: wired the audit into the Wireframe `run` and
  `create-user`'s `run_create_user`, and added the missing context to the
  legacy call site. Obligation V6 discharged by inspection: audit at
  `src/server/wireframe/mod.rs:145` precedes `.bind(bind_addr)` at line 166;
  the audit at `src/server/admin.rs:78` precedes `apply_migrations` at line 81
  and `create_user` at line 82; the legacy audit at
  `src/server/legacy/mod.rs:169` precedes `apply_migrations` at line 176.
- [x] (2026-10-13) EP-M3: updated `README.md`, `docs/developers-guide.md`,
  `docs/users-guide.md`,
  `docs/supporting-both-sqlite3-and-postgresql-in-diesel.md`, and the
  now-inaccurate startup-audit sentence in `docs/design.md`. Committed as
  `b4144a1`.
- [x] (2026-10-13) EP-M2/M3 verification: the standalone
  `cargo check --features sqlite --all-targets` was abandoned because it sat
  queued in the shared compile-admission pool for over ten minutes; the same
  coverage is delivered by the `make lint` and `make test` gates, which compile
  every target with all features. Not restarted, so as not to lengthen the
  shared queue.
- [x] (2026-10-13) First full gate run (commit `6429e45`): `nixie` passed;
  `fmt`, `check-fmt`, `markdownlint`, and `lint` failed. All four failures were
  in this change's own new material: two MD013 over-long lines holding inline
  SQL in this ExecPlan, one typos false positive against a hash-shaped test
  string in `src/db/audit.rs`, and the `module_max_lines` 400-line ceiling on
  `src/server/legacy/mod.rs`. Fixed in `ceccd66`.
- [x] (2026-10-13) Second full gate run on `ceccd66`: all gates pass. See
  `Outcomes & retrospective` for the evidence.
- [ ] (EP-M4) Push the branch, open the draft pull request, and run the
  CodeRabbit review.

## Surprises & discoveries

- Observation: the remote branch
  `origin/issue-63-verify-and-document-dependency-on-sqlite-s-json1-extension-and-recursive-ctes`
  does not yet exist; `git ls-remote` returns nothing. Evidence:
  `git ls-remote git@github.com:leynos/mxd.git 'refs/heads/issue-63*'` printed
  no rows. Impact: there is no upstream to track until the first push, so
  `git branch --set-upstream-to` must happen after the push rather than before.
  The branch is renamed locally now so the eventual push uses the required name.
- Observation: the ambient shell rewrites GitHub SSH URLs to HTTPS through
  `GIT_CONFIG_KEY_*` entries and a Lody credential helper, and that helper
  refuses the push. Evidence: `git ls-remote ssh://git@github.com/...` printed
  `[Lody GitHub] {"stage":"acquire","code":"personal_auth_missing"}`. Impact:
  all remote Git and GitHub operations must run through a wrapper that strips
  `GIT_CONFIG_*`, `GH_TOKEN`, `GITHUB_TOKEN`, `BASH_ENV`, `GIT_SSH`, and
  `GIT_SSH_COMMAND`, and pins `GIT_SSH_COMMAND=/usr/bin/ssh`. Wrappers were
  written to `/tmp/mxd-git-clean.sh` and `/tmp/mxd-gh-clean.sh`; both were
  verified to authenticate as `leynos`.
- Observation: `json_each` returns `text` for the `value` column when iterating
  a JSON array of strings, and `integer` for `key`. Evidence:
  `sqlite3 :memory:
  "SELECT key, typeof(value) FROM json_each('[\"a\",\"b\",\"c\"]')"`
  printed `0|text`, `1|text`, `2|text`, and `typeof(key)` printed `integer`.
  Impact: the typed row struct can use `i32` for `key` and `String` for `value`
  directly; the `CAST(value AS TEXT)` in the query is belt-and-braces rather
  than load-bearing, but it makes the intent explicit and costs nothing.
- Observation: JSON1 is not unconditionally present in a 3.35.0 library. JSON
  became a default-on part of the SQLite core in 3.38.0 and can be removed with
  the `SQLITE_OMIT_JSON` compile-time option, while before 3.38 it required the
  separate `SQLITE_ENABLE_JSON1` build option. Impact: the 3.35.0 version floor
  and the JSON1 probe test genuinely different things, and neither subsumes the
  other. Both are kept, and the documentation states them as two separate
  requirements.

## Decision log

- Decision: enforce the 3.35.0 floor in Rust rather than relying on the
  `returning_clauses_for_sqlite_3_35` Cargo feature to fail the build.
  Rationale: the Cargo feature is a compile-time declaration of intent. It does
  not stop a binary compiled against an older `libsqlite3.so` from being run on
  a machine that has one; `libsqlite3-sys` links the system library
  dynamically. A runtime check is the only thing that covers the deployed case.
  Date/Author: 2026-10-13, agent.
- Decision: test the failure paths by extracting pure validators and testing
  those, rather than by trying to obtain a broken SQLite library in CI.
  Rationale: no CI runner has a JSON1-less SQLite, and the source order already
  places the audit before `bind`, so a subprocess startup-failure test would
  test the wrong thing cheaply. Pure validators give deterministic coverage of
  every rejection branch. Date/Author: 2026-10-13, agent.
- Decision: follow the existing PostgreSQL audit's error style —
  `DieselError::QueryBuilderError(Box<io::Error>)` carrying a formatted message
  — rather than introducing a new error enum. Rationale: consistency with the
  adjacent audit, and the signature is fixed at `QueryResult<()>`, so a new
  enum would have to be flattened back into `DieselError` anyway. Date/Author:
  2026-10-13, agent.
- Decision: do not add an executor trait to abstract the three probe queries.
  Rationale: an explicit instruction in the task, and the probes have different
  row shapes and different failure semantics; a trait would add indirection
  without removing duplication. Date/Author: 2026-10-13, agent.
- Decision: place the `#[cfg(test)]` module inline in `src/db/audit.rs`
  provided it keeps the file under 400 lines; otherwise split it to
  `src/db/audit/tests.rs`. Rationale: keeps related code colocated as
  `AGENTS.md` prefers, while respecting the hard 400-line limit. Date/Author:
  2026-10-13, agent. Outcome: the inline module leaves `src/db/audit.rs` at 361
  lines, so no split was needed.
- Decision: drop the version-string test case carrying a 64-character build
  hash and use a plain `3.46.1 2024-08-13 09:16:08` case instead. Rationale:
  `make markdownlint` flagged a two-letter fragment of the hash as a
  misspelling, and adding a `typos.local.toml` exception for a hash that the
  audit will never see would be a worse trade. `sqlite_version()` returns only
  `major.minor.patch`; the build hash comes from `sqlite_source_id()`, so the
  case was unrealistic as well as noisy. The trailing-text case is still
  covered. Date/Author: 2026-10-13, agent.
- Decision: shorten the `setup_database` doc comment rather than extract an
  `audit_backend` helper to fit the legacy module back under the 400-line
  ceiling. Rationale: the helper form measured 412 lines, because a helper must
  carry its own doc comment and a `let _ = (conn, database);` to satisfy the
  mutually exclusive `sqlite` and `postgres` cfg combinations; shortening a doc
  comment left the module at 398 lines and changed no behaviour. Date/Author:
  2026-10-13, agent.

## Outcomes & retrospective

Status: COMPLETE. Delivered in commits `00d023a` (audit), `6429e45`
(enforcement at every entry point), `b4144a1` (documentation), `ceccd66`,
`4082d18`, `c21e1f9`, `af46a46`, and `88f4774` (gate fixes and plan upkeep).
Pushed as head `88f4774`; draft pull request
<https://github.com/leynos/mxd/pull/656>.

### What was achieved

A user who runs the legacy server, the Wireframe server, or the `create-user`
subcommand against a SQLite library that is older than 3.35.0, lacks the JSON1
`json_each` function, or lacks recursive CTE support now gets an error naming
the missing capability, before the listener binds and before any row is
written. Previously only the version floor and a `json('{}')` scalar check
existed, on the legacy path alone. The documentation states all three
requirements in `README.md`, `docs/developers-guide.md`, `docs/users-guide.md`,
and `docs/supporting-both-sqlite3-and-postgresql-in-diesel.md`, and
`docs/design.md` no longer describes the old, weaker check.

### Gate evidence

All deterministic gates pass on content `88f4774`, whose only delta from
`af46a46` is one word and one line wrap inside this ExecPlan:

| Gate                       | Result                                |
| -------------------------- | ------------------------------------- |
| `make fmt`                 | pass                                  |
| `make check-fmt`           | pass                                  |
| `make markdownlint`        | pass (`typos.toml` unmodified)        |
| `make nixie`               | pass                                  |
| `make lint`                | pass, zero warnings                   |
| `make test-sqlite`         | 502 passed                            |
| `make test-wireframe-only` | 500 passed                            |
| `make test-postgres`       | 497 passed                            |
| `make test-verification`   | 69 passed, 1 skipped                  |
| `make test-concurrency`    | 7 passed                              |
| `make test-fuzz-harness`   | 8 passed                              |
| `make test-doc`            | 5 passed, 4 ignored                   |
| `make test-rstest-bdd-ui`  | times out at the 300 s cap; see below |

The only non-passing target is `test-rstest-bdd-ui`. It is an environment
failure, not a defect in this change:

- `tests/rstest-bdd-ui/Cargo.toml` depends on `rstest`, `rstest-bdd`,
  `rstest-bdd-macros`, `rstest-bdd-harness-tokio`, and `trybuild`. It does not
  depend on `mxd`, and no file under `tests/rstest-bdd-ui/` mentions `mxd`, so
  nothing in this diff can influence it.
- Its captured stderr shows `Blocking waiting for file lock on package cache`
  and uncached compile-admission queueing; the trybuild `.stderr` comparison
  stage is never reached, and no diff, `mismatch`, or `EXPECTED`/`ACTUAL` text
  appears in the log.
- The machine was heavily loaded throughout: load average peaked near 105 with
  18 concurrent cargo jobs, and the managed sccache server was unreachable, so
  every compilation ran uncached.
- The target runs last in `make test`, so its timeout aborts the chain. The
  four sub-targets it skipped were run individually and all pass, as recorded
  in the table above.

### Reconciliation with the conformance basis

- `ISSUE-63-action-audit` -> EP-M1 -> satisfied; the validator cases in
  `src/db/audit.rs` pass and the real-connection `test_audit_features` proves
  the typed rows deserialize from the linked library.
- `ISSUE-63-action-audit` -> EP-M2 -> satisfied; all three entry points audit
  before binding or writing (V6, by inspection at
  `src/server/wireframe/mod.rs:144-145` before `:166`, `src/server/admin.rs:78`
  before `:81` and `:82`, and `src/server/legacy/mod.rs:166-168` before `:173`).
- `ISSUE-63-doc-requirements` -> EP-M3 -> satisfied; the four target documents
  plus the corrected `docs/design.md` sentence.
- `ISSUE-63-acceptance` -> EP-M4 -> satisfied; gate logs under `/tmp`, remote
  head matching `git rev-parse HEAD`, draft PR #656.

No discovery falsified an upstream assumption. The two discoveries that shaped
the work — JSON1 remaining separable from the version floor, and the Diesel
feature being the source of that floor — are recorded above and are reflected
in both the code and the documentation. No deviation from the approved plan was
needed, so no deviation proposal is raised.

### What would be done differently

The first full gate run failed four of its six targets, and all four failures
were in newly authored material rather than in the implementation: two
over-long Markdown lines, one spelling false positive, and one module breaching
the 400-line ceiling by a single line. Running `make fmt`, `make check-fmt`, and
`make markdownlint` after each commit — they are cheap and do not compile Rust
— would have caught all four immediately instead of after a full gate cycle. The
`make lint` and `make test` gates are the expensive ones and cannot
practically be run per commit, but the cheap Markdown and spelling gates can.

A second lesson: three separate spelling failures came from this plan quoting
the offending text while explaining it, and from the audit's own test data
carrying a hash-shaped literal. Test fixtures and plan prose should avoid
embedded machine-generated-looking strings, or the spelling gate will keep
firing on them.

## Context and orientation

mxd is a Hotline protocol server written in Rust. It supports two database
backends selected by Cargo feature: `sqlite` (default) and `postgres`. Exactly
one must be enabled; `src/db/connection.rs` and `src/lib.rs` both emit
`compile_error!` if neither or both are set.

The relevant pieces, by path:

- `src/db/audit.rs` — the module this work extends. It holds
  `audit_sqlite_features` (gated on the `sqlite` feature) and
  `audit_postgres_features` (gated on `postgres`). The PostgreSQL function is
  the template: it loads `SELECT version()` into a typed `QueryableByName` row,
  parses the version in Rust, and returns a
  `DieselError::QueryBuilderError(Box<io::Error>)` carrying an explanatory
  message when the version is unsupported or unparsable.
- `src/db/mod.rs` — re-exports `audit_sqlite_features` under
  `#[cfg(feature = "sqlite")]`.
- `src/db/connection.rs` — defines `DbConnection`. For SQLite it is
  `diesel_async::sync_connection_wrapper::SyncConnectionWrapper<SqliteConnection>`;
  for PostgreSQL it is `diesel_async::AsyncPgConnection`. It also defines
  `DbPool` and `establish_pool`.
- `src/db/files.rs` — the reference for the typed raw-SQL pattern used by the
  new probes. Its private `file_node_id_from_path` declares a local
  `#[derive(QueryableByName)]` struct with a `#[diesel(sql_type = ...)]`
  attribute per field and reads it with `.get_result(conn).await`.
- `src/server/legacy/mod.rs` — the legacy Tokio runtime. Its private
  `setup_database` already calls `audit_sqlite_features`, then conditionally
  `audit_postgres_features`, then `apply_migrations`, all under one pool
  checkout, before the caller binds.
- `src/server/wireframe/mod.rs` — the Wireframe runtime.
  `WireframeBootstrap::run` establishes the pool, then binds, then announces
  readiness. There is no database audit on this path today.
- `src/server/admin.rs` — shared administrative commands. `run_create_user`
  establishes a connection, applies migrations, then inserts the user. There is
  no audit on this path today.
- `src/db/tests/file_node_tests/sqlite.rs` — holds `test_audit_features`, which
  runs the audit against a real in-memory SQLite database built by the
  `migrated_conn` fixture. This is the success-path test and is retained.
- `Cargo.toml` — the `sqlite` feature enables
  `diesel/returning_clauses_for_sqlite_3_35`, which is what makes 3.35.0 the
  floor. There is a standalone `returning_clauses_for_sqlite_3_35` feature
  mirroring the Diesel one.
- `clippy.toml` — sets `cognitive-complexity-threshold = 9`. A function with
  more than nine points of cognitive complexity fails the lint.

Terms used in this plan: a *probe* is a SQL query whose result the audit
inspects, as opposed to a query whose mere execution success is taken as proof.
A *typed row* is a Rust struct deriving `QueryableByName` that Diesel populates
from named columns. A *plateau* is a repository state that is correct and
coherent on its own.

The three entry points distinguish as follows. The legacy server already
audits; it needs only an added error context. The Wireframe server and the
`create-user` command do not audit and must gain the check. All three must fail
before binding or writing.

## Conformance basis

This plan has no upstream Terms of Reference or technical-design revision. The
governing artefacts are:

- The GitHub issue `leynos/mxd#63`, whose required action is "Audit SQLite on
  application start-up if used" and whose documentation task is to update
  `README.md` and the developer onboarding documentation with the version
  requirement.
- The task breakdown supplied with the issue, comprising Task 1 (extend the
  audit), Task 2 (enforce at three entry points), and Task 3 (test and
  document).
- Repository standards: `AGENTS.md` (code style, file length, testing, spelling,
  error handling) and `docs/documentation-style-guide.md`.
- Outside sources treated as axioms, whose documented interfaces the repository
  code depends on: the SQLite JSON1 documentation (JSON became core and
  default-on in 3.38.0, removable via `SQLITE_OMIT_JSON`), and the Diesel
  feature documentation for `returning_clauses_for_sqlite_3_35` (requires
  SQLite 3.35.0 or later).

Trace links:

```plaintext
ISSUE-63-action-audit        -> EP-M1 -> src/db/audit.rs tests (validator cases)
ISSUE-63-action-audit        -> EP-M2 -> legacy, wireframe, admin call sites
ISSUE-63-doc-requirements    -> EP-M3 -> README.md, docs/developers-guide.md,
                                         docs/users-guide.md,
                                         docs/supporting-both-sqlite3-and-postgresql-in-diesel.md
ISSUE-63-acceptance          -> EP-M4 -> gate logs, pushed branch, draft PR
```

## Verification plan

This change introduces one non-trivial invariant and three lemmas. It
introduces no concurrency, no arithmetic over unbounded values, and no
persisted-format change, so bounded model checking and formal proof are not
proportionate; parameterized and example-based tests are.

Obligation V1 (invariant): *the audit accepts exactly those SQLite libraries
that report a version at or above 3.35.0 and answer both probes as expected.*
Method: parameterized `rstest` cases over version strings plus example-based
tests over probe rows. Rationale: the input space is a small finite partition
(above floor, at floor, below floor, unparsable, empty) with distinct semantics
per class, which is exactly the shape a table test covers and a property test
would not improve on. Domain: version strings drawn from the five classes;
probe rows that match and that mismatch. Artefact: the `#[cfg(test)]` module in
`src/db/audit.rs`. Evidence: `cargo test` per the commands below; before
implementation the tests fail to compile because the validators do not exist,
and after implementation they pass. Discharge condition: all cases pass and
each rejection case asserts the error message names the capability.
Non-vacuity: the cases include both accepting and rejecting examples in every
class, so a validator stubbed to always return `Ok` fails the rejecting cases
and a validator stubbed to always reject fails the accepting cases. This is the
seeded-fault control; it is exercised explicitly during the red stage.

Obligation V2 (lemma): *a version string of the form `major.minor.patch` with
optional trailing text parses to the correct triple, and any string that does
not have that prefix is rejected rather than defaulting to a supported
version.* Method: parameterized cases. Rationale: a silent default here would
convert "unknown library" into "assumed good", which is the failure mode that
matters. Domain: `"3.35.0"`, `"3.45.1"`, `"3.34.1"`, `""`, `"abc"`. Artefact:
as V1. Evidence: the `""` and `"abc"` cases must produce a message containing
`unable to parse sqlite version`. Non-vacuity: `"3.45.1"` and `"3.35.0"` are
satisfying witnesses of the accepting branch; `"3.34.1"` is a witness of the
below-floor branch; `""` and `"abc"` witness the unparsable branch.

Obligation V3 (lemma): *a `json_each` result other than the exact expected
three-row array — too few rows, wrong order, wrong key, wrong value — is
rejected with a message naming the JSON1 capability.* Method: example-based
tests over constructed row vectors. Rationale: the real library in CI always
answers correctly, so only constructed rows can exercise rejection. Domain:
matching rows `[(0,"a"),(1,"b"),(2,"c")]`; truncated `[(0,"a")]`; wrong key
`[(1,"a"),...]`; wrong value. Artefact: as V1. Evidence: rejection cases must
produce a message containing `SQLite JSON1 json_each probe failed`.
Non-vacuity: the matching vector is the satisfying witness for acceptance; the
four mismatching vectors each fail a different predicate, so a validator that
checks only the row count is caught by the wrong-key and wrong-value cases.

Obligation V4 (lemma): *a recursive-CTE result other than `[1, 2, 3]` is
rejected with a message naming the recursive-CTE capability.* Method:
example-based tests. Rationale: as V3. Domain: `[1,2,3]` and `[1]`. Artefact:
as V1. Evidence: the `[1]` case must produce a message containing
`SQLite recursive CTE probe failed`. Non-vacuity: `[1,2,3]` witnesses
acceptance, `[1]` witnesses rejection, and a validator that ignores its
argument fails one of the two.

Obligation V5 (end-to-end, success path only): *the extended audit succeeds
against a real, migrated, in-memory SQLite database.* Method: the existing
`test_audit_features` test unchanged. Rationale: it is the only check that the
typed row structs actually deserialize from the real library; the unit tests
cannot catch a `sql_type` attribute that does not match what SQLite returns.
Domain: the `migrated_conn` fixture. Artefact:
`src/db/tests/file_node_tests/sqlite.rs`. Evidence: the test passes under
`make test-sqlite`. Non-vacuity: this test would fail if the `QueryableByName`
attributes were wrong, so it is a genuine control on the unit tests being
self-consistent.

Obligation V6 (ordering): *the audit runs before the socket binds and before
any row is written.* Method: source inspection rather than a test. Rationale: a
subprocess test would require a broken SQLite library, which no CI runner has;
the property is structural. Noted as a residual gap: this obligation is
discharged by review, not by an automated check, and the ExecPlan records that
explicitly rather than implying test coverage exists. Domain: the three entry
points. Artefact: the diffs to `src/server/legacy/mod.rs`,
`src/server/wireframe/mod.rs`, and `src/server/admin.rs`. Evidence: in
`src/server/wireframe/mod.rs` the audit block appears before the
`server.bind(bind_addr)` statement; in `src/server/admin.rs` the audit call
appears before `apply_migrations`. Non-vacuity: not applicable to a structural
check; stated as such.

Axioms relied upon, not verified here: that the SQLite library answers
`SELECT sqlite_version()` and the two probes according to its documented
interface; that Diesel's `QueryableByName` maps `Integer` to `i32` and `Text` to
`String` for the SQLite backend; and that `pool.get()` in the Wireframe path
propagates a pool error rather than panicking. The first is exercised at its
boundary by V5 against a real library; the second is likewise exercised by V5.

## Plan of work

Work proceeds in four stages. Each stage ends with a validation step; do not
begin the next stage if the current one fails.

Stage A (done): understanding. Read the three entry points, the audit module,
the typed raw-SQL pattern, the four documentation targets, and the build
configuration. Probed the intended queries with the `sqlite3` CLI. No code
changed.

Stage B: red tests for EP-M1. Add a `#[cfg(test)]` module to `src/db/audit.rs`
declaring the private validators this plan requires:

```rust
#[cfg(feature = "sqlite")]
fn ensure_supported_sqlite_version(raw: &str) -> Result<(u32, u32, u32), DieselError>;

#[cfg(feature = "sqlite")]
fn validate_json_each_rows(rows: &[(i32, String)]) -> Result<(), DieselError>;

#[cfg(feature = "sqlite")]
fn validate_recursive_cte_rows(rows: &[i32]) -> Result<(), DieselError>;
```

Write the parameterized cases named in `Verification plan` against those
signatures before implementing the bodies. Run the focused test and record the
failure. The failure will be a compile error (`cannot find function`), which is
the honest red for a new internal API; then temporarily implement each
validator as `Ok(...)` and re-run to confirm the rejecting cases fail. Record
both transcripts. Restore the stubs before stage C.

Stage C: green implementation for EP-M1, then EP-M2. Implement the validators
and rewrite `audit_sqlite_features` to run three checks in order:

1. Version. `SELECT sqlite_version() AS version` into a local
   `#[derive(QueryableByName)]` struct with a single
   `#[diesel(sql_type = Text)] version: String` field, read with
   `.get_result(conn).await?`. Parse with a private helper that accepts a
   leading `major.minor.patch` and ignores any trailing text, then compare
   against `(3, 35, 0)`.
2. JSON1. Load a local `#[derive(QueryableByName)]` struct with
   `#[diesel(sql_type = Integer)] key: i32` and
   `#[diesel(sql_type = Text)] value: String`, read with `.load(conn).await`:

   ```sql
   SELECT key, CAST(value AS TEXT) AS value
   FROM json_each('["a","b","c"]') ORDER BY key
   ```

   Validate that the rows are exactly `(0,"a"), (1,"b"), (2,"c")` in order.
3. Recursive CTE. Load a local `#[derive(QueryableByName)]` struct with
   `#[diesel(sql_type = Integer)] x: i32`, read with `.load(conn).await`:

   ```sql
   WITH RECURSIVE c(x) AS (
       SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 3
   )
   SELECT x FROM c ORDER BY x
   ```

   Validate that the rows are exactly `[1, 2, 3]`.

Every query failure and every row mismatch becomes
`DieselError::QueryBuilderError(Box::new(std::io::Error::other(...)))`. The
version messages are `unable to parse sqlite version: {raw}` and
`sqlite version {raw} is not supported (require >= 3.35.0)`. The probe messages
are `SQLite JSON1 json_each probe failed: {err}` and
`SQLite recursive CTE probe failed: {err}`, where `{err}` is either the
underlying query error or a description of the rows found. Do not use
`.execute()` for the probes; the whole point is to read and check their
results. Correct the doc comment so it says the function returns an error (not
that it "dies") and lists all three checks.

Then, in `src/server/wireframe/mod.rs`, insert after the
`let pool = establish_pool(&config.database).await.context(...)?;` statement:

```rust
#[cfg(feature = "sqlite")]
{
    let mut conn = pool.get().await.context("failed to get db connection")?;
    crate::db::audit_sqlite_features(&mut conn)
        .await
        .context("SQLite capability audit failed")?;
}
```

The braces scope the checkout so it is released before the bind that follows.
Do not add migrations or a PostgreSQL audit here.

In `src/server/admin.rs`, insert between `DbConnection::establish(...)` and
`apply_migrations(...)`:

```rust
#[cfg(feature = "sqlite")]
crate::db::audit_sqlite_features(&mut conn)
    .await
    .context("SQLite capability audit failed")?;
```

In `src/server/legacy/mod.rs`, add `.context("SQLite capability audit failed")`
to the existing call. Keep the audit-then-migrations-then-bind order.

Stage D: documentation and validation for EP-M3 and EP-M4. Four files:

- `README.md`: rewrite the SQLite paragraph to require SQLite 3.35.0 or later,
  the JSON1 extension (`json_each`), and recursive CTEs. State that the legacy
  server, the Wireframe server, and `create-user` inspect the linked library
  before serving requests or writing data, and that a missing capability causes
  the command to exit with an error.
- `docs/developers-guide.md`: in the prerequisites section, state that source
  builds link the system SQLite library and therefore need its development
  files, such as `libsqlite3-dev` on Debian-derived systems. Show
  `pkg-config --modversion sqlite3` as the way to check the version that will
  be linked. Warn that the version reported by the `sqlite3` command-line tool
  does not prove what the linked library provides. Identify the startup audit
  as the authoritative check.
- `docs/users-guide.md`: state the runtime requirement for system-linked
  binaries.
- `docs/supporting-both-sqlite3-and-postgresql-in-diesel.md`: explain that the
  `returning_clauses_for_sqlite_3_35` Diesel feature sets the 3.35.0 floor and
  that the startup audit checks it.

Then run the gates in the order given in `Concrete steps`, which is sequential
by repository policy, and finally push and open the draft pull request.

## Milestones and plateaus

EP-M1 — a stronger SQLite audit with deterministic failure coverage.

- Outcome: `audit_sqlite_features` checks the version floor, `json_each`, and
  recursive CTEs, each with a result check; its failure paths are covered by
  parameterized tests that pass; `src/db/audit.rs` still compiles under both
  `sqlite` and `postgres` feature sets.
- Requirements and gaps: ISSUE-63-action-audit (audit half).
- Acceptance evidence: the validator test names in the `#[cfg(test)]` module of
  `src/db/audit.rs` pass under `make test-sqlite`, and the file remains under
  400 lines.
- Conformance check:
  - Requirements satisfied: the audit proves all three capabilities.
  - Design still followed: the PostgreSQL audit is untouched; the error style
    matches it.
  - Upstream assumptions still valid: JSON1 provenance and the Diesel version
    floor are recorded in `Surprises & discoveries` and match the sources.
  - No unapproved public interface change: the signature is unchanged.
  - No unapproved dependency or persisted-format change.
  - Trace links current.
- Recovery: `git revert` the milestone commit; the previous audit still
  compiles and passes its existing test.
- Remaining gaps: the audit is not yet called on the Wireframe or `create-user`
  paths.
- Compatibility decision: none. The function is application-internal.

EP-M2 — the audit enforced on every entry point.

- Outcome: the Wireframe server and `create-user` audit the linked library
  before binding and before writing; the legacy path reports a context on
  failure.
- Requirements and gaps: ISSUE-63-action-audit (enforcement half).
- Acceptance evidence: `make test-wireframe-only` passes, and source inspection
  confirms the ordering described in obligation V6.
- Conformance check:
  - Requirements satisfied: all three entry points audit before side effects.
  - Design still followed: `establish_pool`, migrations, and the PostgreSQL
    branch are unchanged.
  - Upstream assumptions still valid.
  - No unapproved public interface change.
  - No unapproved dependency or persisted-format change.
  - Trace links current.
- Recovery: `git revert` the milestone commit; the audit simply stops being
  called on the two new paths.
- Remaining gaps: documentation still describes the old, weaker check.
- Compatibility decision: none.

EP-M3 — documentation matches the code.

- Outcome: the four documents state the requirement, the way to check it, and
  the audit's coverage, in en-GB-oxendict spelling, without claiming bundling
  or describing release packaging.
- Requirements and gaps: ISSUE-63-doc-requirements.
- Acceptance evidence: `make markdownlint` and `make nixie` pass; the four files
  contain the required statements.
- Conformance check:
  - Requirements satisfied: README and onboarding docs updated.
  - Design still followed.
  - Upstream assumptions still valid: the 3.35.0 floor is attributed to the
    Diesel feature, not to JSON1.
  - No unapproved public interface change.
  - No unapproved dependency or persisted-format change.
  - Trace links current.
- Recovery: `git revert` the milestone commit.
- Remaining gaps: none.
- Compatibility decision: none.

EP-M4 — validated, pushed, and reviewable.

- Outcome: every gate passes, the branch is pushed and tracking `origin`, and a
  draft pull request is open with the required title, body, and references.
- Requirements and gaps: ISSUE-63-acceptance.
- Acceptance evidence: gate logs under `/tmp/`, the remote head matching
  `git rev-parse HEAD`, and the pull request URL.
- Conformance check:
  - Requirements satisfied: all three tasks delivered.
  - Design still followed.
  - Upstream assumptions still valid.
  - No unapproved public interface change.
  - No unapproved dependency or persisted-format change.
  - Trace links current.
- Recovery: the pull request is a draft, so it can be closed without loss; the
  branch can be reset and re-pushed.
- Remaining gaps: none within the issue.
- Compatibility decision: none.

## Concrete steps

All commands run from the repository root
`/home/leynos/.lody/repos/github---leynos---mxd/worktrees/9ebd1336-1cfe-4db7-953a-f5dd4c5acdcc`.

Focused test loop during development:

```sh
RSTEST_TIMEOUT=225 cargo test --features sqlite audit 2>&1 | tee /tmp/test-audit-issue-63.out
```

Commit gate, delegated to the `scrutineer` subagent so its bulk output stays
out of the planning context. The gates are sequential by repository policy;
never run two of them at once:

```sh
make fmt
make check-fmt
make markdownlint
make nixie
make lint
make test
```

Backend-specific validation:

```sh
make test-sqlite
make test-wireframe-only
make test-postgres
```

Remote operations, always through the clean-environment wrappers:

```sh
/tmp/mxd-git-clean.sh push git@github.com:leynos/mxd.git \
  HEAD:refs/heads/issue-63-verify-and-document-dependency-on-sqlite-s-json1-extension-and-recursive-ctes

/tmp/mxd-git-clean.sh fetch git@github.com:leynos/mxd.git \
  "+refs/heads/issue-63-verify-and-document-dependency-on-sqlite-s-json1-extension-and-recursive-ctes:refs/remotes/origin/issue-63-verify-and-document-dependency-on-sqlite-s-json1-extension-and-recursive-ctes"

/tmp/mxd-git-clean.sh branch --set-upstream-to=origin/issue-63-verify-and-document-dependency-on-sqlite-s-json1-extension-and-recursive-ctes

/tmp/mxd-git-clean.sh ls-remote git@github.com:leynos/mxd.git \
  refs/heads/issue-63-verify-and-document-dependency-on-sqlite-s-json1-extension-and-recursive-ctes
```

The leading `+` on the fetch refspec is deliberate: it keeps the refresh
working after a rebase or an amended force-push.

Expected transcript for the version probe, matching what was observed with the
system `sqlite3` 3.46.1 during reconnaissance:

```plaintext
$ sqlite3 :memory: "SELECT sqlite_version() AS version;"
3.46.1
$ sqlite3 :memory: "SELECT key, CAST(value AS TEXT) AS value FROM json_each('[\"a\",\"b\",\"c\"]') ORDER BY key;"
0|a
1|b
2|c
$ sqlite3 :memory: "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 3) SELECT x FROM c ORDER BY x;"
1
2
3
```

## Validation and acceptance

Acceptance is expressed as behaviour.

- Running `make test-sqlite` passes. The new validator cases in
  `src/db/audit.rs` fail before the validators are implemented and pass after.
- Running `make test-wireframe-only` passes, proving the Wireframe path still
  compiles and behaves without the legacy networking feature.
- Running `make test-postgres` passes, proving the PostgreSQL build is
  unaffected by the gated changes.
- Running `make lint` passes with `-D warnings`, proving the new code satisfies
  Clippy including the repository's pedantic set and the
  `cognitive-complexity-threshold = 9` ceiling in `clippy.toml`. If the
  three-probe function exceeds that ceiling, extract each probe into its own
  small async helper.
- Running `make check-fmt`, `make markdownlint`, and `make nixie` passes.

Red-Green-Refactor evidence to record in `Artefacts and notes`:

- Red: run the focused test after adding the test module and before implementing
  the validators. Expect a compile error naming the missing validator
  functions. Then stub each validator to return the accepting value and re-run;
  expect the rejecting cases to fail. Both transcripts are the red evidence.
- Green: implement the validators and the three probes, re-run the focused
  test, expect all cases to pass.
- Refactor: with the tests green, extract probe helpers if
  `audit_sqlite_features` breaches the cognitive-complexity ceiling, then
  re-run the focused test and `make lint`.

Quality criteria: tests pass on all three feature configurations; every
validator obligation V1 through V5 is discharged; V6 is discharged by recorded
source inspection; lint and formatting gates pass; no benchmark or security
scan is required for this change.

## Idempotence and recovery

Every step in this plan is re-runnable. `make fmt` rewrites files in place and
is safe to repeat. The tests are deterministic and use in-memory databases, so
re-running them creates no state. The push is not idempotent against a
force-push, but a plain re-push after an amended commit is safe on a branch no
one else has based work on; if that stops being true, escalate rather than
force-pushing over another author's commits.

If a gate fails, read the captured log rather than re-running the gate
immediately. Each gate writes to `/tmp` with a name of the form
`/tmp/<action>-<project>-<branch>.out`.

## Artefacts and notes

### Test transcripts

- Red (stubbed validators, 13 rejecting cases failing and 4 accepting passing):
  `/tmp/test-audit-issue-63-red.out`
- Green (all 22 validator cases passing):
  `/tmp/test-audit-issue-63-green.out`
- Real-connection `test_audit_features` (discharges V5):
  `/tmp/test-audit-e2e-issue-63.out`

### Gate logs

The first full run was made against `6429e45`; its four failures were fixed in
`ceccd66`. Logs are named `/tmp/<gate>-<project>-<branch>.out`, where the
project slug expands to
`9ebd1336-1cfe-4db7-953a-f5dd4c5acdcc-issue-63-verify-and-document-dependency-on-sqlite-s-json1-extension-and-recursive-ctes`:

| Gate                | Exit | Notes                         |
| ------------------- | ---- | ----------------------------- |
| `make nixie`        | 0    | Passed at `6429e45`.          |
| `make fmt`          | 2    | Fixed in `ceccd66`.           |
| `make check-fmt`    | 2    | Fixed in `ceccd66`.           |
| `make markdownlint` | 2    | Fixed in `ceccd66`.           |
| `make lint`         | 2    | Fixed in `ceccd66`.           |
| `make test`         | 2    | See below; not a code defect. |

The `make test` target chains eight sub-targets. The three nextest suites all
passed — 497 tests under the `postgres` profile, then 524 and 522 under the
`default` profile. The run then failed in `test-rstest-bdd-ui`, a trybuild
compile-contract test, which timed out at 300 s. Its captured stderr contains
only `Blocking waiting for file lock on package cache`, so the timeout was
caused by another agent holding Cargo's package-cache lock, not by an assertion
or `.stderr` mismatch in this change. At the time the machine's load average
was about 105 with 18 concurrent cargo jobs. The target is unaffected by this
change: it compiles `tests/rstest-bdd-ui`, and nothing in this diff touches
that crate.

### Pull request

Draft PR: <https://github.com/leynos/mxd/pull/656>, base `main` at `df48e51`,
head `4082d18`. Verified after creation: `isDraft` is true, the title carries
`(#63)`, the body carries `Closes #63`, a `## References` section holding the
Lody session URL, and the Claude Code attribution line.

### Push record

Pushed to `git@github.com:leynos/mxd.git` via `GIT_SSH_COMMAND=/usr/bin/ssh`,
because the worktree's `origin` is an HTTPS URL and the ambient environment
rewrites SSH URLs to HTTPS. The tracking ref was then refreshed explicitly:

```sh
git fetch git@github.com:leynos/mxd.git \
  "+refs/heads/BRANCH:refs/remotes/origin/BRANCH"
git branch --set-upstream-to=origin/BRANCH BRANCH
```

Confirmed afterwards that `git rev-parse HEAD`,
`git ls-remote git@github.com:leynos/mxd.git refs/heads/BRANCH`, and the
upstream all name `4082d18a6ac3bcb3b81fc530465535a6ed218aa3`.

## Interfaces and dependencies

At the end of EP-M1 the following must exist in `src/db/audit.rs`, all gated on
`#[cfg(feature = "sqlite")]`:

```rust
pub async fn audit_sqlite_features(conn: &mut DbConnection) -> QueryResult<()>;

fn ensure_supported_sqlite_version(raw: &str) -> Result<(u32, u32, u32), DieselError>;

fn validate_json_each_rows(rows: &[(i32, String)]) -> Result<(), DieselError>;

fn validate_recursive_cte_rows(rows: &[i32]) -> Result<(), DieselError>;
```

`audit_sqlite_features` keeps its public signature. The three helpers are
private to the module and exist so that the failure paths can be tested without
a broken SQLite library. No new public type, trait, or dependency is introduced.

No new crate is added. The work uses `diesel`, `diesel_async`, and `rstest`,
all already present.

## Revision note

2026-10-13: initial draft, written after reconnaissance but before any code
change. Records the discovered remote-branch absence, the credential-routing
workaround, the verified `json_each` column types, and the JSON1-version
provenance that the documentation claims will rest on.

2026-10-13: EP-M1 through EP-M3 implemented and committed (`00d023a`, `b4144a1`,
`6429e45`). The first full gate run exposed four failures in this change's own
new material, all fixed in `ceccd66`; the ExecPlan was resynchronized in
`4082d18`. EP-M4 pushed the branch and opened draft PR #656.
`Outcomes & retrospective` remains open pending the closing gate report.
