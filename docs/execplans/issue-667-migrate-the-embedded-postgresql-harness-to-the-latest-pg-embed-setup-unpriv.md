# Migrate the embedded-PostgreSQL harness to the latest pg-embed-setup-unpriv (#667)

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

No `PLANS.md` was found in the repository root.

## Purpose / big picture

Move mxd's embedded-PostgreSQL test harness from `pg-embed-setup-unpriv`
`0.5.2` to the latest release, `0.6.3`, and retire the serial nextest profile
that `0.5.2` forced on the PostgreSQL leg. mxd is the last consumer still on
`0.5.x`; axinite, diesel-cte-ext and corbusier have all moved on.

Success is observable when:

- `Cargo.lock` holds exactly one `pg-embed-setup-unpriv` version, `0.6.3`.
- `make test-postgres` runs at default nextest parallelism, with the `pg-embed`
  test group and its `all()` override removed, and passes with zero timeouts.
- Both GitHub Actions PostgreSQL legs and the coverage lane are green on the
  pull request, and the post-merge coverage run on `main` is green.
- `docs/developers-guide.md` describes the parallel model that actually runs,
  not the serial one it replaces.

## Constraints

- One pull request. One locked `pg-embed-setup-unpriv` version.
- Do not change the SQLite legs.
- Do not raise a CI or nextest timeout to absorb a regression. A slower
  parallel run is a finding to report, not a budget to bump.
- Keep `PostgresTestDb::{new, new_async, new_from_template}` signatures and the
  bootstrap-versus-set-up error mapping unchanged.
- Keep the fixed `PG_PASSWORD`; an explicit password overrides the stored one
  in `0.6`, and the local throwaway clusters rely on that.
- Do not pin `PG_RUNTIME_DIR` and `PG_DATA_DIR` together for `test-postgres`.
- Keep every source file under 400 lines, and use en-GB-oxendict spelling.
- If coverage drops, lower the stored baseline in the same pull request and put
  the old and new figures in the body. Only SQLite carries a ratchet.

## Tolerances (exception triggers)

- Blocker: stop if the `TimeZone` failure cannot be mitigated without a
  `postgresql_archive` `0.22.0` upgrade, which needs Rust 1.94 and a release of
  `pg-embed-setup-unpriv` that does not exist yet.
- Dependency: stop if the migration needs a crate beyond the version bumps this
  plan names.
- Validation: stop if the same gate fails twice without new diagnostics.
- Isolation: stop if default-parallel `make test-postgres` shows cross-test
  contamination rather than a throughput or connection-limit problem.

## Risks

- Risk: the migration alone does not clear the `TimeZone` lane failure, because
  the defect is in what the harness downloads rather than in the harness crate.
  Severity: high. Likelihood: confirmed. Mitigation: pin the bundle with
  `PG_VERSION_REQ`; see the first Surprise below.

- Risk: default parallelism exhausts the cluster's connection budget.
  pg-embed-setup-unpriv caps a test cluster at `max_connections = 20`.
  Severity: medium. Likelihood: medium. Mitigation: measure, then set
  `PG_MAX_CONNECTIONS` if the evidence calls for it, and say why.

- Risk: per-cluster data directories under `/var/tmp/pg-embed-<uid>` consume
  disk in proportion to concurrency. Severity: low. Likelihood: medium.
  Mitigation: check disk use and look for orphaned directories and servers
  after a parallel run.

- Risk: removing `#[file_serial]` uncovers a cross-process race that the group
  was masking for a reason other than the shared data directory. Severity:
  medium. Likelihood: low. Mitigation: the run either passes or it does not;
  diagnose before applying any remedy.

## Surprises & Discoveries

- Observation: the October `TimeZone` failure is a bundle defect, not a
  harness defect, and upgrading the harness does not fix it. The `18.6.1`
  release tarball contains 257 tar hard links, among them
  `share/timezone/UTC -> share/timezone/UCT` and `Etc/UTC -> UCT`.
  `postgresql_archive` before `0.22.0` extracts directories, files and symlinks
  and silently skips hard links, so `share/timezone/UTC` never exists.
  PostgreSQL then rejects `SET TIME ZONE 'UTC'`, which is what diesel's
  `PgConnection::establish` sends. Evidence: a hard-link-skipped extraction of
  the `18.6.1` tarball reproduces
  `ERROR: invalid value for parameter "TimeZone": "UTC"` exactly, while the
  intact tree accepts it. Impact: the target release cannot be judged by its
  version number alone.

- Observation: the fix is unreachable from any published
  `pg-embed-setup-unpriv`. `postgresql_archive` `0.22.0` (21 October) added
  hard-link extraction, but `0.6.3` requires `postgresql_embedded` `0.20.2`,
  which requires `postgresql_archive ^0.20.4`. The `.20` line has no release
  carrying the fix. Impact: the migration needs an in-repo working-bundle pin,
  not just a version bump.

- Observation: the August `18.6.0` bundle is intact and still downloadable at
  its original upload. Its Linux asset was created and last updated on
  2026-08-15, was built `--with-system-tzdata=/usr/share/zoneinfo`, ships no
  `share/timezone` and has no hard links. Evidence: `SET TIME ZONE 'UTC'`
  succeeds against it, and `created_at` equals `updated_at` on the asset, so it
  was not replaced in the October rebuild wave. Impact: `PG_VERSION_REQ` can
  name it exactly.

- Observation: `PG_VERSION_REQ` is applied verbatim as a `VersionReq`, so a
  caret requirement would resolve to the newest matching release, which is the
  broken `18.6.1`. Evidence: `apply_version` in `src/lib.rs` parses the
  variable straight into `settings.version`, and release selection takes the
  highest semver match across all pages. Impact: the pin must be exact, as in
  `=18.6.0`.

- Observation: the `0.6.3` API that mxd uses is source-identical to `0.5.2`.
  `src/cluster/handle.rs`, `src/cluster/lifecycle.rs`, `src/cluster/guard.rs`,
  the `hash_directory` helper and the guard split are unchanged, and the
  `async-api` and `diesel-support` feature names are unchanged. Impact: the
  port is a version bump and a pin, not a rewrite.

- Observation: an unpinned local `make test-postgres` on `0.5.2` failed at test
  31 of 497 with `failed to connect to admin database: db error`, while an
  isolated rerun of that same test passed. Evidence: the failure is the
  `TimeZone` defect surfacing through diesel, and it is intermittent locally
  because the bundle is resolved per run. Impact: the earlier baseline was not
  a valid before-measurement; the pinned baseline replaces it.

- Observation: a second, distinct failure appeared once in the parallel leg:
  one wireframe routing case failed at 8.3 s with
  `postgresql_embedded::start() failed` and no `pg_ctl` detail. It has not
  recurred in four consecutive full parallel runs (34.9 s, 39.8 s, 37.0 s, 37.3
  s, all 499/499). Cause not established. The harness now renders the whole
  error-cause chain, because `BootstrapError` displays only its outermost
  context and the `pg_ctl` diagnostic was being discarded; the next occurrence
  will name the real cause rather than only the operation. Resource exhaustion
  is ruled out: 125 GB RAM with 80 GB available, `ulimit -n` 524288, 32000
  semaphore arrays, 62 GB free on `/dev/shm`, and no orphaned servers or data
  directories after any run.

- Observation: `0.6.3` has a race when many clusters bootstrap at once in one
  root, and it is upstream, not mxd's. `claim_slot` in
  `src/bootstrap/prepare/cluster_slot.rs` sweeps dead slots before taking the
  `ClaimGuard` that orders claims, and `lock_new_slot` creates a slot's lock
  file and only then `flock`s it. A concurrent process can list the
  just-created lock in that window, take it first in `take_dead_lock`, and
  sweep the slot as dead; the owner's `try_lock` then returns `WouldBlock` and
  the start fails with `slot lock <path> is already held`. Evidence: the first
  parallel run failed once at 6.5 s with exactly that message; the lock file
  named in the error did not exist afterwards while twelve sibling slot locks
  did, which is what a sweep leaves behind. Upstream `main` still sweeps before
  taking the guard, and no upstream issue reports it. The crate classifies this
  failure as bootstrap kind `Other`, so its own three-attempt shared-cluster
  retry does not catch it. Impact: `test-util` retries a lost slot race eight
  times with a short growing backoff, in both the sync and async start paths. A
  retry claims a new slot name, so it does not repeat the same race.

- Observation: the upstream shared-cluster initializer caches its first
  failure, and its retry classifier does not recognize the slot-contention
  error. The template path therefore needs a fallback when the shared
  initializer reports that specific error. `shared.rs` leaves the upstream
  initializer in place for normal starts and uses a separate `OnceLock` for
  fallback initialization. Fresh bootstrap attempts run inside that lock before
  a fallback result is cached. The fallback retains the worker guard, registers
  the shutdown hook, and only then forgets the combined guard, in keeping with
  the upstream lifetime pattern. Unrelated primary failures stay terminal and
  retain their full cause chain. Evidence: injected-contention tests cover
  fallback success and reuse, cached terminal fallback failure, and unrelated
  primary failure diagnostics; a real-cluster test exercises template creation
  and database cloning. The tests do not reproduce the production race schedule
  or verify cleanup at process exit.

- Observation: the `rstest-bdd-ui` leg timed out twice at its existing 300 s
  budget, then passed once its trybuild fixtures were warm. This is host
  compile-admission starvation, not a defect in the diff. The leg and its
  `package(rstest-bdd-ui-tests)` timeout override are untouched by this branch,
  and trybuild compiles its fixture crates inside the test process, so it
  inherits `RUSTC_WRAPPER=build-limits-rustc` and queues behind every other
  agent's builds. At the time 239 requests were queued behind 4 slots held by
  other repositories, and the fixture build advanced only 22 crates per 300 s
  attempt. Evidence: under plain `cargo test`, which has no nextest
  slow-timeout, the same test passed in 989.01 s with both fixtures correct;
  the next `make test-rstest-bdd-ui` then passed in 157.067 s. Impact: none in
  code. Record the mechanism so the same timeout is not misread as a
  regression, and do not raise the 300 s budget for it.

## Decision Log

- Decision: target `0.6.3`, not `0.7.0`. Rationale: crates.io reports `0.6.3`
  as `max_version` and `newest_version`, and upstream releases stop at
  `v0.6.3`. Date/Author: 2026-10-24.

- Decision: pin the working bundle in-repo with an exact `PG_VERSION_REQ`.
  Rationale: the defect is in the downloaded bundle, no published harness
  release can select the fixed extractor, and the fixed extractor needs Rust
  1.94 through `postgresql_archive` `0.22.0`. Pinning to the August `18.6.0`
  build is MSRV-neutral, reproducible and evidence-backed. Date/Author:
  2026-10-24.

- Decision: keep the pin in the Makefile and the two workflows rather than in
  Rust. Rationale: `make test-postgres` and `make warm-postgres` are the
  interface, CI calls the Makefile targets, and a Rust-side override would
  change the harness helper's semantics for callers who never asked for one.
  Date/Author: 2026-10-24.

- Decision: preserve the upstream shared-cluster initializer and cache a
  separate fallback only for slot contention. Rationale: the upstream
  initializer caches failures before mxd can retry them; the fallback retries
  fresh starts before caching its own result while retaining shared lifetime
  and shutdown handling. Date/Author: 2026-10-27.

## Progress

- [x] (2026-10-24) Confirmed `0.6.3` as the target release; no `0.7.0` exists.
- [x] (2026-10-24) Confirmed the `TimeZone` cause by experiment, and confirmed
  that no published harness release can reach the fix.
- [x] (2026-10-24) Confirmed the August `18.6.0` bundle is intact, unreplaced
  and accepts `SET TIME ZONE 'UTC'`.
- [x] (2026-10-24) Recorded the serial `0.5.2` baseline under the pin: 497/497
  passed, 0 timeouts, 288.3 s of test time, 300 s wall.
- [x] (2026-10-24) Posted the confirmed cause to mxd #665 and cross-referenced
  upstream #358, including the exact-pin correction.
- [x] (2026-10-24) Posted the design and the pin to mxd #667 for agreement
  before changing code.
- [x] (2026-10-24) Bumped both manifests. `bd03fb0`: root dev-dependency on the
  harness removed (no imports), `test-util` set to `0.6.3` with `async-api`.
  The harness ported with no source changes. The lock commit for the rustls
  advisories is folded in, because it clears `cargo audit`.
- [x] (2026-10-24) `eda3f3c`: pinned `PG_VERSION_REQ=18.6.0` in the Makefile and
  both workflows. Without it 0.6.3 selects 18.6.1 and every connection fails.
- [x] (2026-10-24) `e0881ad`: deleted the `pg-embed` test group and its `all()`
  override, and all 22 `file_serial` attributes. The wireframe in-process
  `serial_test::serial` stays.
- [x] (2026-10-24) `7cd4d20`: added `tests/postgres_harness_isolation.rs` and
  extended both workflow contracts. All 260 contracts pass.
- [x] (2026-10-24) Measured the parallel leg: 499/499 passed at default
  parallelism, 0 timeouts, 34.873 s of test time against 288.3 s serial, an
  8.3x reduction. Slowest test 8.820 s (`wireframe_memory_budgets::…`); no
  PostgreSQL test is in the slowest five, and the 15.774 s cold-start outlier
  from the serial baseline is gone because per-cluster starts overlap. The
  standalone `make test-postgres` gate reproduced it in 34.313 s.
- [x] (2026-10-24) Confirmed the intermittent `start()` failure did not recur
  across seven consecutive green parallel runs. Cause still not established;
  the cause-chain diagnostics are in place for the next occurrence.
- [x] (2026-10-24) Rewrote the developers' guide, users' guide and the
  harness users' guide, and added
  `pg-embed-setup-unpriv-v0-6-0-migration-guide.md`.
- [x] (2026-10-24) All local gates green at `4176f80`: `make check-fmt`,
  `make lint` (three Clippy passes, 0 errors), `make typecheck` (three passes),
  `make test` (postgres 499/499 in 37.271 s, sqlite 502/502, wireframe-only
  500/500, rstest-bdd UI passed in 157.067 s, verification 69/69, fuzz-harness
  8/8, concurrency 7/7 plus doctests, doc), `make test-postgres` (499/499 in
  34.313 s, 0 timeouts), `make markdownlint` (0 issues), `make nixie`,
  `make spelling`, `make test-workflow-contracts` (260 passed).
- [x] (2026-10-27) Addressed the review follow-up at `77db522`: added the
  separate shared-cluster fallback in `shared.rs`, deterministic tests for
  contention, fallback success and reuse, terminal failure caching, and
  unrelated primary errors, plus a real-cluster template clone test. Corrected
  the three migration documentation findings. CodeRabbit's reassessment found
  the earlier findings addressed and no remaining in-scope implementation
  defect.
- [x] (2026-10-27) Hosted CI run
  [40418836060](https://github.com/leynos/mxd/actions/runs/40418836060) passed
  at `77db522eb34aba215ff01b3fad369228216445c3`. Its PostgreSQL coverage job
  passed 638 tests with one skipped in 75.052 s; logs show the fallback, retry,
  isolation, time-zone, and cleanup tests passed. The PostgreSQL build leg
  skips its test step, so the coverage job is the PostgreSQL execution
  evidence. CodeScene passed at the same head. Local gates were not rerun for
  this head.
- [x] (2026-10-27) Final review repairs are present at `d4b5e96`: added
  retry/fallback metrics and bounded tracing, source-chain regression tests for
  terminal failures, a property test for mixed retry sequences, and workflow
  contract coverage for PostgreSQL target overrides and exact helper versions.
  Reconciled the design, developer, and migration documentation. The manual
  documented-function check remains 45/45 (100%); this is not a refreshed
  provider score.
- [x] (2026-10-27) Hosted CI run
  [40430679039](https://github.com/leynos/mxd/actions/runs/40430679039) passed
  at `d4b5e9613bf8ac7962c1a7ca328013f47b733a02`. PostgreSQL coverage passed 640
  tests with one skipped in 80.141 s; SQLite coverage passed 633 with one
  skipped. Logs confirm retry, property, fallback, isolation, time-zone, and
  cleanup tests passed. CodeScene succeeded. The PostgreSQL build leg skips its
  test step, so coverage is the PostgreSQL execution evidence. Local gates were
  not rerun for this head.
- [x] (2026-10-30) Cleared review and merged PR #668
  ([leynos/mxd#668](https://github.com/leynos/mxd/pull/668)). CodeRabbit
  approved the head `1322df390d284ac6967689fe46e726e40f433fd1` on base
  `da9c7da3da9b4f69a90fb7d38d405085f5a891a8`, with no unresolved threads and
  every required check green. The stack-aware merge landed squash commit
  `0c0b853d365b4cf0c9389de56375097b55557cb3` on `main` and retargeted the
  dependent PR #673 onto `main`.
- [x] (2026-10-30) Post-merge `coverage-main.yml` run
  [41101023280](https://github.com/leynos/mxd/actions/runs/41101023280) passed
  at `0c0b853d365b4cf0c9389de56375097b55557cb3`: PostgreSQL coverage passed 641
  tests with one skipped in 53.831 s, SQLite coverage passed 633 with one
  skipped in 213.264 s, and CodeScene accepted the upload. The job waited about
  65 minutes for an Ubicloud runner during the GitHub Actions degradation of
  17:50-18:32 UTC, so its 33-minute wall time is not comparable with the
  quiet-host figures above.

## Outcomes & Retrospective

Delivered: `pg-embed-setup-unpriv` 0.6.3 is the only locked version, the
PostgreSQL leg runs at default nextest parallelism with the `pg-embed` test
group and its `all()` override gone, and the post-merge coverage run on `main`
is green. The parallel leg runs 499 tests in about 35 s of test time against
288.3 s serial at 0.5.2, an 8.3x reduction, with zero timeouts.

What the migration turned out to need:

- The version bump itself needed no harness source changes. The blocking defect
  was the upstream time-zone bug, and the exact `PG_VERSION_REQ=18.6.0` pin is
  what clears it; 18.6.1 still fails every connection.
- `0.6.3` sweeps dead cluster slots before taking the guard that orders slot
  claims, and it creates a slot lock file before locking it. Concurrent
  bootstraps can therefore have a just-created lock removed as stale, and the
  loser sees `slot lock ... is already held`. The retry and shared-cluster
  fallback in `test-util/src/postgres/` exist for that race; the diagnostics
  render the whole cause chain so a `pg_ctl` failure is not hidden behind an
  outer message.
- Only the first cold cluster start in a run is slow (about 7 s); warm starts
  take about 2 s.

Lessons worth carrying forward:

- A test that compiles fixture crates inside itself needs a timeout budget sized
  against a loaded host, not a quiet one. The `rstest-bdd-ui-tests` trybuild
  case hit its 300 s ceiling once under full compile-admission contention and
  passed in 153.9 s on the warm rerun.
- A stacked merge is not a plain merge: `gh pr merge` is refused, and the
  stack-aware operation retargets the dependent PR onto the base branch. Check
  the successor's base and CI immediately afterwards instead of assuming it
  still points at the branch that just landed.

## Context and orientation

mxd is a Rust workspace. Its PostgreSQL tests run against an embedded cluster
and have no route to an external server. The current harness implementation and
regression coverage live in `test-util/src/postgres/`:

- `embedded.rs` starts clusters and implements retry handling for slot
  contention; its `retry_tests` module covers retry sequences.
- `common.rs` holds the URL and name newtypes, the template hash, the schema
  reset, and the drop-on-teardown helper.
- `shared.rs` preserves the upstream shared-cluster initializer and provides a
  separately cached fallback for slot-contention failures.
- `mod.rs` presents `PostgresTestDb` and the `rstest` fixtures.
- `test_errors.rs` supplies nested errors used to test source-chain reporting.

At the start of this migration, the root manifest also had an unused
dev-dependency; that was removed, leaving the optional dependency in
`test-util/Cargo.toml`, gated by its `postgres` feature. The current
`.config/nextest.toml` retains a `postgres` profile for cold-start timeout
headroom, but no longer has the former single-slot `pg-embed` group or `all()`
override. PostgreSQL tests run at default parallelism. The `Makefile` exposes
`test-postgres` and `warm-postgres`. Two workflows run the PostgreSQL lane, and
Python contract tests assert their shape.

The `TimeZone` blocker arrives through diesel: `PgConnection::establish` sends
`SET TIME ZONE 'UTC'` when it opens a connection, so a bundle that is missing
its time-zone aliases fails the first database connection rather than any
PostgreSQL-specific assertion. That is why the failure looked like a harness
fault for as long as it did.

## Plan of work

Stage A: record the blocker and agree the pin.

Post the experimental result to mxd #665, cross-referencing upstream #358,
which reached the same cause independently. State the onset evidence honestly:
mxd's CI failures attributed to this cause start on 19 October, the day after
`18.6.1` was published, and the earlier coverage failures on 14 and 16 October
have a different cause. Get agreement on the exact pin before changing code,
because it is a deliberate, temporary workaround for an upstream defect.

Stage B: bump and port.

Update both manifests to `0.6.3`, keeping `async-api` in `test-util` and
`diesel-support` at the root, delete the unused root dev-dependency, refresh
the lockfile, and confirm one version remains. Port the helper, which should be
close to a no-op given that the used API is unchanged, and add the regression
tests: two fixtures alive at once must see isolated databases, and
`SET TimeZone = 'UTC'` must succeed. Update the `cargo binstall` pins in both
workflows to `0.6.3`, and give the Makefile and both workflows the bundle pin.

Stage C: remove the serialization and measure.

Delete the `pg-embed` group, its `all()` override and every
`#[file_serial(postgres_embedded_setup)]`, keeping the wireframe tests'
in-process `serial_test::serial`. Run `make test-postgres` at default
parallelism, record wall time, the slowest five tests and the timeout count,
and compare against the baseline. Diagnose before remedying if it fails.

Stage D: document and validate.

Rewrite the developers' guide sections that describe the serial model, refresh
the vendored upstream guides, align the users' guide, add the target-version
migration guide and link it from `docs/CONTENTS.md`, update the workflow
contracts, run the full gate sequence, and commit atomically.

## Concrete steps

All commands run from the worktree root.

1. Baseline, under the pin:

   ```sh
   PG_VERSION_REQ='=18.6.0' make test-postgres
   ```

2. Dependency bump, then confirm one locked version:

   ```sh
   cargo tree -d | rg pg-embed-setup-unpriv
   ```

3. Prove a SQLite-only build carries no pg-embed:

   ```sh
   cargo tree --no-default-features --features "sqlite test-support" | rg pg-embed
   ```

4. Parallel measurement, after removing the group:

   ```sh
   make test-postgres
   ```

5. The gate sequence, in order, stopping at the first failure:

   ```sh
   make check-fmt
   make lint
   make typecheck
   make test
   make test-postgres
   make markdownlint
   make nixie
   make spelling
   make test-workflow-contracts
   ```

Long output goes to `/tmp/<action>-mxd-<branch>.out` through `tee`.

## Validation and acceptance

Acceptance criteria:

- Dependencies: one `pg-embed-setup-unpriv` version in `Cargo.lock`, `0.6.3`,
  and a SQLite-only build that does not carry it.
- Behaviour: `PostgresTestDb` keeps its constructors and error mapping; two
  concurrent fixtures see isolated databases; the cluster honours
  `SET TimeZone = 'UTC'`.
- Parallelism: `make test-postgres` runs at default parallelism with zero
  timeouts and no cross-test contamination; any retained cap is justified by
  measurement and carries a comment saying why.
- Contracts: the workflow contract tests assert the removed group and the
  matching `binstall` pins.
- Documentation: the developers' guide describes the model that runs.
- CI: both PostgreSQL legs and the coverage lane are green.

Quality method: run the gate sequence above and read each log for the failure,
not just the exit status.

## Idempotence and recovery

Every change is source-level and re-runnable. The pin is an environment default
in version-controlled files, so a re-run reproduces it exactly. If a gate
fails, fix the reported cause and re-run that gate alone. Keep commits small so
a partial rollback stays surgical, and revert the pin separately from the
migration if the pin turns out to be wrong.

## Artefacts and notes

Baseline, serial `0.5.2` under `PG_VERSION_REQ='=18.6.0'`, 2026-10-24:

- 497 tests run, 497 passed, 0 skipped, 0 failed, 0 timeouts.
- Summary wall time in nextest: 288.3 s. Process wall time: 300 s.
- Slowest five:

  | Seconds | Test                                                                                             |
  | ------- | ------------------------------------------------------------------------------------------------ |
  | 15.774  | `db::schema_alignment_tests::postgres_tests::postgres_category_names_are_bundle_scoped`          |
  | 7.101   | `wireframe_memory_budgets::stalled_fragmented_request_is_disconnected_when_continuation_resumes` |
  | 6.799   | `session_privileges_bdd::session_privileges_authenticated_b`                                     |
  | 5.916   | `wireframe_compat_guardrails_bdd::wireframe_compat`                                              |
  | 5.054   | `session_privileges_bdd::session_privileges_authenticated_b` (case)                              |

  Only the first is a PostgreSQL test. It is a cold cluster start; the warm
  starts in the same run take about 2 s.

The first attempt, unpinned, was not a valid baseline: it failed at test 31 of
497 with `failed to connect to admin database: db error`, which is the same
bundle defect surfacing through diesel. Pinning `PG_VERSION_REQ='=18.6.0'`
removed it, which is also the evidence that the pin, not the harness version,
clears the lane.

CI reference figures from the last green run on 13 October (run 38354647170):

- Coverage job, 06:04:44 to 06:22:35, about 17m51s.
- "Download the embedded PostgreSQL binaries", 53s.
- "Generate coverage for Postgres", 06:17:16 to 06:21:32, about 4m16s.
- `build-test (postgres)`, 06:04:46 to 06:21:34, about 16m48s, tests skipped.

## Interfaces and dependencies

Dependencies to end with:

- `test-util/Cargo.toml`: `pg-embedded-setup-unpriv`, package
  `pg-embed-setup-unpriv`, `version = "0.6.3"`, optional,
  `features = ["async-api"]`.
- Root `Cargo.toml`: the dev-dependency is removed.

Interfaces that must not change:

- `test_util::postgres::PostgresTestDb::{new, new_async, new_from_template}`
- `test_util::postgres::{postgres_db, postgres_db_fast}`
- `PostgresTestDbError::{EmbeddedBootstrapFailed, EmbeddedInitFailed}`

## Revision note

Initial draft, 2026-10-24, after confirming the `TimeZone` cause by experiment,
establishing that no published harness release reaches the fix, and validating
an exact `PG_VERSION_REQ=18.6.0` pin as the MSRV-neutral remedy.
