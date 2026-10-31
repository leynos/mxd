# Migrate mxd to OrthoConfig v0.10.0

This ExecPlan is a living document for issue
[#669](https://github.com/leynos/mxd/issues/669).

Status: ACCEPTED — the plan's implementation and validation obligations were
accepted on 2026-10-31 (see the acceptance record below); PR #670 awaits review
convergence and the authorized merge.

## ExecPlan acceptance record

Accepted: 2026-10-31. Accepted by the delivery owner of PR #670, which is the
authority this plan appoints for its own acceptance; implementation completion
alone was not treated as acceptance.

Evidence on which the acceptance rests:

- every obligation in "Validation and acceptance" below was executed and its
  result recorded, including the full gate sequence, the feature lanes, the
  build-time man-page contract, and the imported-document hash comparison;
- an independent assessment of the implementation accepted the migration
  obligations (Assessment-ID `EXECPLAN-669-8B681FE-001`) and left only the
  delivery record and verification gaps open;
- the two verification gaps it raised were closed: the scanner invariant now
  carries property tests, and the hosted Whitaker lanes pass at the current
  head;
- every pre-merge check has a disposition, and the current CI run for the
  candidate is green.

Remaining after acceptance: only merge of PR #670 and post-merge verification,
which do not change the plan's obligations.

One follow-up is recorded but deliberately left out of this PR. Running
`make spelling` on an up-to-date host rewrites `typos.toml`: the pinned
`typos-config-builder` v0.1.3 now emits two `extend-ignore-re` patterns for
`tokio` attribute arguments and drops the earlier broad pattern that ignored
every `\bflavor` occurrence. `main` and this branch carry the same pre-change
copy, so the drift predates this work and is unrelated to the migration.
Committing it here would add unrelated generated content to a reviewed diff, so
it will be regenerated and committed in a separate base pull request after #670
lands.

## Purpose / big picture

Move both mxd's runtime and shared build-time CLI consumers from the Git-tagged
OrthoConfig v0.3.0 API to the published v0.10.0 release. Keep configuration
precedence, discovery, localized diagnostics, generated man-page output,
process exit codes, and output streams observable and covered by tests. Replace
process-global test setup with release-supported injected sources where it
replaces the existing `figment::Jail` code.

## Constraints

- Preserve defaults < files < environment < explicit CLI precedence, including
  explicit CLI values equal to their defaults.
- Preserve file discovery, subcommand namespaces, localized parse failures,
  generated man pages, exit status, and stdout/stderr contracts.
- Keep upstream documentation imports byte-for-byte equal to commit
  `38812c563e3953a490e3efc202609f37e21a0517`; local notes belong in this plan
  or the linked provenance document.
- Keep OrthoConfig's default features enabled, including `serde_json`, and
  retain the repository's intentional TOML, JSON5, YAML, and metrics choices.
- Do not depend on v0.10.0 roadmap items or add an injection API absent from the
  published release.
- Keep changes at the configuration and CLI adapter boundary. Do not move
  domain responsibilities or add an application-owned configuration port when
  OrthoConfig's source-aware entry points already supply the required seam.
- Do not modify submodules, vendored code, or unrelated work. The baseline
  worktree was clean at `da9c7da3da9b4f69a90fb7d38d405085f5a891a8`.

## Tolerances (exception triggers)

- Scope: changes outside the root manifest, `cli-defs`, `build.rs`, the current
  configuration tests, CI/toolchain metadata, the ExecPlan, and directly
  affected configuration/design/user/developer documentation trigger a scope
  review before continuing. This task explicitly requires user-facing and
  internal documentation to match the migrated CLI and compiler floor.
- Interface: do not add a new public mxd API or compatibility wrapper; update
  the internal callers together.
- Dependencies: add no direct dependency beyond the published OrthoConfig
  crates required by actual imports. Remove a duplicate only after source and
  test evidence proves it is redundant.
- Behaviour: any changed exit code, stdout/stderr routing, discovery outcome,
  localization, generated documentation, or precedence test is a stop condition
  until the contract is restored or the issue's explicit requirement explains
  the difference.
- Iterations: after three focused attempts to resolve a deterministic gate
  failure, record the evidence and reassess the cause before further edits.
- Time: there is no wall-clock limit for this task.

## Risks

- Baseline fact (2026-10-26): the lockfile included OrthoConfig 0.5.0 through
  the PostgreSQL test helper. Severity: medium; likelihood: high. Crates.io
  lists helper v0.6.3 as the latest release, and its dependency API still
  requires `ortho_config ^0.5.0`. Keep the existing helper rather than taking
  unrelated dependency churn that would not remove the duplicate.
- YAML now uses the stricter Saphyr provider. Severity: medium; likelihood:
  medium. Exercise existing YAML fixtures and YAML 1.2 edge cases rather than
  assuming the parser swap is transparent.
- The repository has no declared Rust floor, and the locked full dependency
  graph includes test packages requiring Rust 1.94.0. Severity: medium;
  likelihood: high. Validate the actual targets on 1.94.0 and align the root
  application and shared CLI package metadata with that floor.
- Global Markdown formatting can alter imported upstream copies. Severity:
  low; likelihood: medium. Restore those files from the pinned commit after
  formatting and verify the recorded SHA-256 values.
- Build and test work shares Cargo's cache and compiler admission with other
  repositories. Severity: low; likelihood: medium. Use the shared cache and
  allow healthy queues to make progress; do not bypass admission.
- Fresh `pg-embed-setup-unpriv` PostgreSQL archives can omit the `UTC` alias
  even though `UCT` is present, preventing CI's PostgreSQL lane from starting.
  Severity: medium; likelihood: high for a cold binary cache. Repair the alias
  in both the isolated warmed cache and install, then exercise that behaviour
  in the workflow contract test.
- The current RustSec advisory database flags vulnerable versions already
  present on the baseline branch. Severity: medium; likelihood: high in hosted
  CI. Update only the affected lockfile packages to the advisory's fixed
  versions and explain that targeted graph change.

## Progress

- [x] (2026-10-26 12:17Z) Confirmed clean baseline commit, task branch, open
  issue #669, no existing migration branch or pull request, and issue #574 as
  the superseded v0.9.0 migration.
- [x] (2026-10-26 12:17Z) Fetched `v0.10.0`; tag object and peeled commit match
  the issue's recorded hashes.
- [x] (2026-10-26 12:17Z) Imported and read the requested user's guide and
  migration guides before any implementation edits; recorded provenance and
  link mapping in
  [the provenance record](../ortho-config-v0-10-0-provenance.md).
- [x] (2026-10-26 12:17Z) Read the upstream changelog. Its v0.9.0 section
  explicitly includes v0.8.0 and earlier changes.
- [x] (2026-10-26 12:17Z) Completed read-only baseline inventory: one workspace
  lockfile, eight packages, two direct v0.3.0 consumers, and a separate 0.5.0
  test-helper dependency chain. These are baseline dependency facts, not the
  final migrated graph.
- [x] (2026-10-26 12:25Z; baseline survey) Confirmed that the latest
  published PostgreSQL test helper (v0.6.3) still requires
  `ortho_config ^0.5.0`; the 0.5.0 lockfile entry remains an explained
  transitive dependency.
- [x] (2026-10-26 12:25Z) Counted ten existing `AppConfig` loading tests in
  `src/server/cli.rs`; all currently run inside the `figment::Jail` helper.
- [x] (2026-10-26 12:33Z) Confirmed the application has no Fluent catalogue,
  localizer, or `LocalizedParse` call. The current output contract is Clap and
  OrthoConfig fallback diagnostics, so validation will exercise their text,
  exit status, and streams through a child process rather than inventing a
  translation layer.
- [x] (2026-10-26 12:39Z) Recorded baseline `make test` as red after 17m35s
  of compilation: PostgreSQL stopped at
  `db::schema_alignment_tests::postgres_tests::postgres_category_names_are_bundle_scoped`
  with `failed to connect to admin database: db error`. Of 497 tests across 33
  binaries, 30 passed, 1 failed, 0 were skipped, and 466 were not run. The
  remaining lanes were run separately; full output is at
  `/tmp/test-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`.
- [x] (2026-10-26 12:45Z) Separate baseline `make test-sqlite` ran 502 tests:
  501 passed and one wireframe XOR scenario failed because
  `mxd-wireframe-server` could not bind an address already in use. The
  wireframe-only baseline passed all 500 tests. The SQLite and wireframe-only
  logs use the `/tmp/test-sqlite-59318af2-...` and
  `/tmp/test-wireframe-only-59318af2-...` filenames.
- [x] (2026-10-26 12:56Z) `make test-verification` passed 69 tests with one
  expected skip. `make test-rstest-bdd-ui` timed out its sole compile-fail test
  after 300 seconds while the compiler pool was full and queuing other builds;
  it needs an evidence-led retry before final validation.
- [x] (2026-10-26 12:59Z) `make test-fuzz-harness` passed all eight tests.
- [x] (2026-10-26 13:02Z) `make test-concurrency` passed 7 nextest tests and
  all 7 doctests; `make test-doc` passed 5 and reported 4 ignored. The baseline
  inventory is complete. PostgreSQL admin connection, SQLite bind collision,
  and UI compile timeout failures remain tied to the unmodified baseline; no
  concrete environment correction has been identified.
- [x] (2026-10-26 13:07Z) `make fmt` completed; `make check-fmt`,
  `make markdownlint`, and `make nixie` passed sequentially. All five imported
  upstream SHA-256 checksums still match the provenance record. The uppercase
  `docs/CONTENTS.md` is the repository's documentation catalogue and will be
  updated alongside `docs/repository-layout.md`.
- [x] (2026-10-26 17:41Z) Committed the five immutable upstream documents,
  provenance and local repository documentation in `05f8c5c`. The final docs
  gates passed sequentially, all imported hashes match, and the final
  documentation CodeRabbit review returned zero findings.
- [x] (2026-10-26 17:41Z) `cargo metadata --locked` showed the full existing
  graph needs Rust 1.94.0 (`postgresql_*` and `sqlx`) and includes
  `cargo-platform` 1.91. Root and `cli-defs` will declare 1.94.0, and each CI
  feature lane will check all targets with that stable compiler.
- [x] (2026-10-26 18:20Z) Updated root and `cli-defs` manifests to crates.io
  OrthoConfig v0.10.0, removed direct Figment/YAML/JSON5/XDG dependencies,
  retained OrthoConfig's defaults and forwarded TOML/JSON5/YAML features. The
  lockfile retains the explained 0.5.0 helper chain. Declared the graph's Rust
  1.94.0 floor and added a stable 1.94.0 all-target check to each CI lane.
- [x] (2026-10-26 18:20Z) Replaced all ten `figment::Jail` tests with isolated
  source-injected tests. The focused CLI suite passes 16 tests, including
  explicit CLI-default precedence, required/malformed/unreadable file failures,
  and `create-user` file/environment/CLI precedence. v0.10 hides the
  derive-generated parser type, so `Cli` keeps it private in the flattened
  field; `CreateUserArgs` now declares its file namespace explicitly.
- [x] (2026-10-26 19:58Z) Process-level tests passed 8/8 for help, unknown
  subcommands, invalid values, missing files selected by `MXD_CONFIG_PATH` and
  `--config-path`, malformed selected files, local dotfile discovery failures,
  and the generated man page. The man-page assertions account for roff's
  escaped hyphens.
- [x] (2026-10-26 18:49Z) Optional-format tests passed with `--features json5`
  (17/17) and `--features yaml` (18/18). Coverage includes JSON5 loading, YAML
  1.2 string scalars, and duplicate keys. Saphyr reports the duplicate as
  `OrthoError::Merge` with the path and parser diagnostic; the regression
  asserts that published error shape.
- [x] (2026-10-26 21:39Z) Candidate deterministic gates passed. The isolated
  aggregate `make test` used
  `PG_DATA_DIR=/var/tmp/mxd-issue-669-pgdata-59318af2` after earlier PostgreSQL
  bootstrap collisions with concurrent worktrees; it passed PostgreSQL 511/511,
  SQLite 516/516, wireframe-only 506/506, UI 1/1, verification 69 passed/1
  expected skip, fuzz 8/8, concurrency 7/7 and 7/7 doctests, and docs 5
  passed/4 ignored. Exit status was 0 in 372 seconds. Log:
  `/tmp/test-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0-isolated-pg.out`.
- [x] (2026-10-26 21:43Z) Final reverse dependency trees confirm both direct
  consumers resolve `ortho_config 0.10.0` from crates.io. The only remaining
  `ortho_config` and `ortho_config_macros` 0.5.0 pair is pulled by
  `pg-embed-setup-unpriv 0.5.2` as a development dependency. The 2026-10-30
  rebase moved that helper to 0.6.3, which pulls the same 0.5.0 pair, so the
  current locked chain is `pg-embed-setup-unpriv 0.6.3` → `ortho_config 0.5.0`
  and the conclusion is unchanged. Rust 1.94.0 identifies as
  `4a4ef493e3a1488c6e321570238084b38948f6db`; the repository nightly identifies
  as `80d0e4be6f15899649ba31669077c59a986f96cc`.
- [x] (2026-10-26 21:47Z) After the final design-note edit, `make fmt`,
  `make check-fmt`, `make markdownlint` (including spelling), `make nixie`, and
  `git diff --check` passed. All five imported-document hashes still match the
  provenance record.
- [x] (2026-10-27 01:58Z) The first source CodeRabbit review completed against
  `5738204fdb73608fdffb4fe920c9e883191a0ff9` and found two minor test-isolation
  risks: injected-source tests could discover a host project file, and the
  child-process project-file test could discover host HOME/XDG files. Commit
  `4cd88c9` selects an empty controlled file for default-value tests, keeps
  home-dotfile discovery as a separate check, and supplies temporary HOME/XDG
  directories to the child process.
- [x] (2026-10-27 02:30Z) All deterministic gates passed after the review fixes:
  `make check-fmt`, all six `make lint` lanes, isolated `make test` (PostgreSQL
  511/511, SQLite 516/516, wireframe-only 506/506, UI 1/1, verification 69
  passed/1 expected skip, fuzz 8/8, concurrency 7/7 plus 7 doctests, docs 5
  passed/4 ignored), focused JSON5 (1/1), focused YAML (2/2), and Rust 1.94.0
  all-target checks for PostgreSQL, SQLite, and wireframe-only.
  `git diff --check` and all five imported-document hash checks also passed.
  Logs use the `/tmp/*-isolation-fixes.out` suffix.
- [x] (2026-10-27 02:35Z) The second source CodeRabbit review completed against
  the full branch at `9d7ca280cf817e2eac2fb85134348cfc01fd247b` with zero
  findings, clearing the two test-isolation concerns from its first review.
- [x] Complete the v0.10.0 applicability matrix with source-level evidence.
- [x] Migrate manifests, lockfile, compiler floor, source imports, loaders, and
  isolated configuration tests.
- [x] Complete behavioural and feature-lane validation, generated-man-page
      checks,
  and final deterministic gates.
- [x] (2026-10-27) Complete milestone reviews, push the branch, and create
  draft PR #670, which closes #669 and supersedes #574. The PR is not yet
  merged.
- [x] Fix the CLI argv scanner and add process-level regressions for a
  subcommand name used as an option value, the `--` terminator, and supported
  option placement around the subcommand.
- [x] Complete the precedence-conflict matrix for migration timeout, database,
  Argon2 time and parallelism, and subcommand credentials. Exercise the
  production password-restoration path with disposable storage; prove that a
  CLI password overrides file/environment values, is excluded from
  serialization, and produces the expected stored password hash.
- [x] Run and record independent `cli-defs` default, no-default, and format
  feature checks outside workspace feature unification. Keep OrthoConfig's
  default features enabled. Add YAML scalar checks for `no`, `on`, `off`,
  `true`, and `false`, and record the formats actually enabled before making
  any disabled-format coverage claim.
- [x] (2026-10-30) Reran the candidate's deterministic gates sequentially on
  the final source bytes. `make check-fmt`, `make typecheck`, `make lint`,
  `make markdownlint`, and `make nixie` passed, and aggregate `make test`
  passed every suite once the embedded PostgreSQL cluster was given a private
  `PG_DATA_DIR`.
- [x] (2026-10-30) Rebased onto `origin/main` at `0c0b853d`, which had advanced
  with the embedded-PostgreSQL harness migration. Only `Cargo.lock` conflicted;
  it was resolved with the target's file and then regenerated with
  `cargo metadata`. `git range-diff` shows eleven commits unchanged and two
  differing only in replaced lockfile hunks. All nine gates were rerun at the
  new head `c2dadc8` and passed.
- [x] (2026-10-30) Published the rewritten head `8b681fe` with a lease-bounded
  force push and obtained a fresh CodeRabbit review there. Its assessment
  confirmed the migration and the four closed gaps and left three items:
  restore hosted Whitaker validation, add range-based verification of the
  scanner invariant, and reconcile this plan's current-state text.
- [x] (2026-10-30) Repaired the hosted Whitaker bootstrap by pinning
  `install-whitaker` to the revision whose installer (0.2.11) provisions a
  verified Clippy snapshot instead of cloning rust-clippy's mutable branch.
- [x] (2026-10-30) Repaired the `warm-postgres` UTC alias loop so it replaces a
  dangling `UTC` link and propagates `find` and `ln` failures instead of
  masking them; both behaviours were exercised in a scratch tree.
- [x] (2026-10-30) Added the two executing contract cases for that repair:
  `test_warm_postgres_replaces_a_dangling_utc_alias` (a seeded broken link must
  end up pointing at `UCT`) and
  `test_warm_postgres_fails_when_an_alias_root_is_missing` (a missing install
  root must fail). The workflow-contract gate rose from 266 to 268 cases.
- [x] (2026-10-30) Extracted a pure `config_args_from` scanner and added a
  `proptest` suite for the token-classification invariant, keeping the
  process-level contracts as the end-to-end specification.
- [x] (2026-10-30) Reran the eleven deterministic gates sequentially. The first
  two sequences failed on Clippy findings in the new code, both fixed in the
  source rather than suppressed: `clippy::indexing-slicing` on `args[..end]` in
  `config_args_from`, now `args.iter().take(end).cloned().collect()`, and
  `clippy::doc_markdown` on an unbackticked `OrthoConfig` in the new test
  module's `//!` header. `make lint` then passed in all three feature sets,
  including Whitaker, and `make test` passed every lane: 1680 nextest cases and
  12 doctests, one nextest case and four doctests skipped or ignored as
  recorded. `make check-fmt`, `make markdownlint`, `make spelling`,
  `make nixie`, `make test-workflow-contracts`, `make test-dependabot-policy`,
  `make check-locked`, and `mbake validate Makefile` passed.
- [ ] Push the repaired head, reconcile the remaining pre-merge rows against the
  live table, and merge only after the PR reaches equilibrium.

## Surprises & discoveries

- Baseline observation (2026-10-26): the root lockfile resolved OrthoConfig
  0.5.0 as well as the directly declared Git-tagged 0.3.0 release. Evidence:
  `cargo tree -i ortho_config@0.5.0 --locked` traces the 0.5.0 copy through
  `pg-embed-setup-unpriv 0.5.2` as a development dependency. Impact: the final
  graph must explain this chain if a compatible PostgreSQL helper release does
  not remove it.
- Observation: the instructions name lowercase `docs/contents.md`, while the
  tracked catalogue is `docs/CONTENTS.md` on this case-sensitive filesystem.
  Evidence: the tracked file list and Makefile formatter include the uppercase
  path. Impact: update that existing catalogue and `docs/repository-layout.md`
  rather than creating a duplicate index.
- Observation: the CodeGraph skill is present but no CodeGraph MCP tools are
  exposed in this session. Evidence: the available tool list and targeted tool
  search returned no `codegraph_*` operation. Impact: use the requested
  targeted source searches and keep file/line evidence in this plan.
- Baseline survey observation (2026-10-26): the newest published
  `pg-embed-setup-unpriv` release still depends on the old OrthoConfig line.
  Evidence: Firecrawl retrieved crates.io metadata for v0.6.3 and its
  dependency list declares `ortho_config ^0.5.0`, the same range as v0.5.2.
  Impact: do not update this test helper merely to chase a single OrthoConfig
  version; retain and explain the dependency chain.
- Observation: the locked full workspace graph includes dependencies declaring
  Rust 1.94.0 (`postgresql_archive`, `postgresql_commands`,
  `postgresql_embedded`, and `sqlx`), plus `cargo-platform` 1.91. Evidence:
  `cargo metadata --locked` against the baseline lockfile. Impact: validate the
  actual targets on 1.94.0 rather than inferring the repository floor from
  OrthoConfig's lower 1.89.0 floor; the existing `nightly-2026-03-26` toolchain
  already exceeds it.
- Observation: `build.rs` only reads Cargo's `OUT_DIR` and `CARGO_PKG_NAME`;
  no `cargo-orthohelp` bridge is installed or invoked by this repository.
  Evidence: `build.rs` has the only application build-time environment reads,
  and repository search found no `cargo-orthohelp` command or metadata table.
  Impact: keep the existing clap-mangen generator and validate its actual
  output under the v0.10 build-time dependency.
- Observation: v0.10 hides the generated parser as `__<Config>Cli`, so the
  former public `AppConfigCli` name no longer exists. Evidence: the first build
  failed at that type reference and the published derive source emits a private
  helper. Impact: `Cli` retains the helper as a private flattened field;
  runtime help and the generated man page retain the shared flags.
- Observation: v0.10's legacy environment-selected config path is only an
  optional discovery candidate. Evidence: the injected missing-path test fell
  through to defaults. Impact: opt into `explicit_mode = "required_exclusive"`,
  preserving first-wins automatic discovery while a selected path remains
  required and suppresses fallback.

## Decision log

- Decision: import the five upstream files from the verified tag's immutable
  commit and preserve their bytes. Rationale: issue #669 requires version-bound
  authoritative material and forbids substituting mutable documentation
  sources. Date/Author: 2026-10-26, Codex.
- Decision: keep OrthoConfig at the existing configuration adapter boundary and
  use its v0.10.0 source-aware APIs for hermetic tests. Rationale: this replaces
  `figment::Jail` without adding an application-owned injection API or
  changing domain dependencies. Date/Author: 2026-10-26, Codex.
- Decision: enable v0.10 `required_exclusive` explicit discovery for
  `AppConfig`. Rationale: a configured `MXD_CONFIG_PATH` or `--config-path`
  must not silently fall through to defaults or another file; automatic
  discovery remains first-wins. Date/Author: 2026-10-26, Codex.
- Decision: remove direct configuration crates only when every remaining
  production and test import is supplied by the v0.10.0 facade or no longer
  needed. Rationale: the older guides document re-exports, but mxd's own
  imports decide whether a direct dependency remains necessary. Date/Author:
  2026-10-26, Codex.
- Baseline decision (2026-10-26): keep `pg-embed-setup-unpriv` at 0.5.2
  unless the migration itself requires an API update. Rationale: crates.io's
  latest 0.6.3 release still depends on `ortho_config ^0.5.0`, so changing this
  helper would add unrelated churn and would not remove the locked 0.5.0 copy.
  Date/Author: 2026-10-26, Codex. Superseded on 2026-10-30: `main` adopted
  0.6.3 through its own harness migration, and rebasing onto it carried that
  version in; the 0.5.0 pair remains, exactly as predicted here.
- Decision: declare Rust 1.94.0 for both `mxd` and `cli-defs`. Rationale: the
  existing locked full feature/test graph already includes crates requiring
  1.94.0; declaring only OrthoConfig's 1.89.0 floor would advertise an
  unsupported compiler for promised targets. Keep the existing dated nightly
  pin and test the full feature matrix on 1.94.0. Date/Author: 2026-10-26,
  Codex.

## Outcomes & retrospective

The source migration and deterministic gates are complete, with a clean
aggregate result after isolating this candidate's embedded PostgreSQL data
directory. The baseline and first candidate PostgreSQL bootstrap failures
occurred while multiple worktrees used the helper's shared default data
directory. The isolated candidate run passed; since the baseline was not rerun
with an isolated directory, shared-directory contention remains the likely
cause rather than a proven one. The SQLite bind collision and UI timeout
occurred on baseline; the migrated SQLite and UI lanes both passed in the final
aggregate run.

The migrated default PostgreSQL lane runs 511 tests versus 497 on baseline
(+14), SQLite runs 516 versus 502 (+14), and wireframe-only runs 506 versus 500
(+6). The delta consists of six additional injected-source configuration
regressions replacing and expanding the ten `figment::Jail` cases, plus eight
child-process CLI contract cases enabled in the PostgreSQL and SQLite lanes.
Optional JSON5 and YAML checks add separate format-specific cases and are
reported above. Verification, fuzz, concurrency, doctest, and documentation
inventories retain their baseline counts and expected skips.

The first delivery milestone produced PR #670 from commit `4fd7a85`; its final
CodeRabbit review returned zero findings. That review predates the acceptance
gaps below, so it does not clear the current candidate. The gaps were closed in
`840224f`, `1ddc716`, and their follow-ups, and the PR is ready rather than
draft at head `8b681fe` on base `0c0b853d`. Prior dependency statements in the
baseline inventory describe the pre-migration graph: the migrated root lockfile
has no direct Git OrthoConfig source, while the remaining 0.5.0
`ortho_config`/`ortho_config_macros` pair is pulled by
`pg-embed-setup-unpriv 0.6.3` as a development dependency.

### Acceptance gaps (closed 2026-10-30; retained as history)

The four items below record each gap as it was raised on 2026-10-30, not work
that remains. All four are closed. The scanner now validates its prefix with
Clap before treating a token as a subcommand, and process-level contract tests
cover command-name option values, `--`, and option placement on either side of
the subcommand. A four-tier precedence matrix and a disposable-storage
credential test exercise the merge and restoration paths end to end. Independent
`cli-defs` feature checks and YAML 1.2 scalar cases passed. Only review
convergence and the authorized merge remain for delivery.

1. **CLI token classification and process coverage.** The retained scanner in
   `src/server/cli.rs` treats any `create-user` token as the subcommand
   boundary. Consequently, a valid value such as `--database create-user` is
   truncated before the second parser sees it. Fix the scanner's distinction
   between option values and subcommands, then cover command-name option values,
   `--`, and supported option placement around the subcommand through the real
   `load_cli()` process path. The existing injected-loader tests do not cover
   this path.
2. **Conflicting precedence layers and credential restoration.** Add the
   missing migration-timeout, database CLI, Argon2 time/parallelism, and
   subcommand password conflict cases. Verify that CLI password input wins over
   file and environment values, remains absent from serialized config, and
   reaches the production restoration path with disposable storage and the
   expected resulting password hash.
3. **Independent `cli-defs` feature coverage and YAML scalars.** Validate the
   package's default, package-level no-default, and format feature combinations
   independently of workspace feature unification. This must not disable
   OrthoConfig's own default features. Test YAML scalars `no`, `on`, `off`,
   `true`, and `false`; record which formats are enabled before asserting
   coverage for disabled formats.
4. **Delivery reconciliation.** The implementation already has a pushed
   draft PR and a zero-finding review at `4fd7a85`, but the new work must be
   gated and reviewed at its final head. Update the PR description and this
   plan with that evidence, then follow the requested babysitting workflow and
   merge only when all checks and review state reach equilibrium.

## Context and orientation

At the migration baseline, the workspace root and `cli-defs/Cargo.toml` both
pinned `ortho_config` to Git tag `v0.3.0`. `cli-defs/src/lib.rs` defines
`AppConfig` and shared CLI structs; the crate is used both by runtime binaries
and the root `build.rs` man-page generator. `src/server/cli.rs` calls
`AppConfig::load_from_iter`, while `src/server/admin.rs` uses the subcommand
merge API. At baseline, configuration tests in `src/server/cli.rs` used
`figment::Jail`, environment mutation, and temporary config files.

At the migration baseline, the root lockfile also contained registry
OrthoConfig 0.5.0 through `pg-embed-setup-unpriv 0.5.2`. The baseline compiler
is
`rustc 1.96.0-nightly (80d0e4be6f15899649ba31669077c59a986f96cc, 2026-03-25)`;
`rust-toolchain.toml` pins `nightly-2026-03-26` with rustfmt, Clippy,
llvm-tools-preview, and rust-analyzer.

## Conformance basis

- Issue #669 is the task's requirements source. Its baseline is commit
  `da9c7da3da9b4f69a90fb7d38d405085f5a891a8`.
- Issue #574 is an open v0.9.0 migration analysis and will be closed as
  superseded by the draft PR for #669.
- Upstream OrthoConfig v0.10.0 tag object is
  `996e5a1c9f22ace4f55bf5dcc196d2c0524262bf`; peeled commit is
  `38812c563e3953a490e3efc202609f37e21a0517`.
- The imported source paths, destinations, checksums, and link mapping are in
  [the provenance record](../ortho-config-v0-10-0-provenance.md).
- There is no separate issue-linked technical design or ADR for this dependency
  migration. The existing [design document](../design.md) and
  [hexagonal-architecture adoption note](../adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md)
  remain context; the plan preserves the current CLI/configuration adapter
  boundary and proposes no architectural change.

### v0.10.0 migration-guide applicability matrix

| Guide change                                                       | Applicability and planned evidence                                                                                                                                                                                                                                                                                 |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Update OrthoConfig, direct macro, and tooling versions together    | Required for root and `cli-defs`; inventory direct macro and tooling use, then inspect manifests and lock graph.                                                                                                                                                                                                   |
| Explicit CLI values win even when equal to defaults                | Required; `explicit_cli_default_overrides_file_and_environment` passes through the source-aware loader. Subcommand file/environment/CLI precedence also passes.                                                                                                                                                    |
| Fluent identifiers and docs IR 2.0                                 | Not applicable: there is no Fluent catalogue, localizer, `LocalizedParse` call, or IR-version consumer. Process tests cover existing Clap/fallback English diagnostics under `LC_ALL=C`.                                                                                                                           |
| `AgentContext.profiles` uses `ProfilesDeclaration`                 | No current `AgentContext` construction was found; repository search confirms no construction site.                                                                                                                                                                                                                 |
| `DocMetadata` literals add `profiles`                              | No library or bridge `DocMetadata` literals were found; `cargo-orthohelp` is not an installed or invoked consumer.                                                                                                                                                                                                 |
| Boolean flags accept `--flag=false`                                | No boolean field exists in shared CLI configuration; repository-wide CLI field search confirms this. The space-separated form remains rejected by v0.10.0.                                                                                                                                                         |
| Named profiles                                                     | Optional and unused; do not reserve `profile` or alter the config schema.                                                                                                                                                                                                                                          |
| Stacked discovery scopes                                           | Not needed; retain first-wins automatic discovery. Set `explicit_mode = "required_exclusive"` because a focused test showed that v0.10's legacy environment selector otherwise falls through on a missing selected path; process tests cover missing selection through both `MXD_CONFIG_PATH` and `--config-path`. |
| Injected environment and file sources                              | Applied: all ten former `Jail` tests use published `_with_sources` entry points, `MapEnv`, and explicit capability file sources. No process environment is mutated in these tests.                                                                                                                                 |
| Inferred string defaults with `cli_default_as_absent`              | No clap `default_value`/`default_value_t` fields were found; do not add the attribute without a demonstrated need.                                                                                                                                                                                                 |
| Agent-native metadata and policy checking                          | Optional; no manifest policy tables or `cargo-orthohelp` invocation exists.                                                                                                                                                                                                                                        |
| Cargo external-subcommand helper                                   | Optional and unrelated to mxd's server CLI; do not adopt.                                                                                                                                                                                                                                                          |
| Standalone `ortho_l10n`                                            | No standalone localization-only consumer exists; retain localization through the facade if needed.                                                                                                                                                                                                                 |
| `cargo-orthohelp` stdout changes and bridge environment allow-list | No bridge invocation exists. Review `build.rs`, which currently reads only Cargo-provided `OUT_DIR` and `CARGO_PKG_NAME`, and validate clap-mangen output.                                                                                                                                                         |
| Localization re-exports, including `FluentValue`                   | The application has no localization runtime or direct `fluent-bundle` dependency. The facade brings `ortho_l10n` and `fluent-bundle` transitively; no local type-identity import or catalogue migration is needed.                                                                                                 |
| Missing, malformed, and unreadable config remain distinct          | Required behaviour; `required_exclusive` preserves the selected-path failure contract. Source-injected tests distinguish all three, and process tests check visible diagnostics, exit status, and streams for missing environment/CLI selections, malformed selections, and discovered dotfiles.                   |

### Earlier migration-guide applicability matrix

| Earlier change                                            | Applicability and planned evidence                                                                                                                                                                      |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.6 dependency re-exports                                | Applied to root and `cli-defs`: removed direct Figment, JSON5, YAML, `uncased`, and `xdg` configuration dependencies. The validator retains TOML directly for its independent validator config parser.  |
| v0.6 declarative discovery and strict failures            | Applied the v0.10 derive's `required_exclusive` policy for selected paths; tests cover absent candidates and missing, malformed, and unreadable selected files.                                         |
| v0.6 Saphyr YAML 1.2 parsing                              | Replaced direct YAML providers with OrthoConfig's Saphyr-backed feature; YAML 1.2 scalar and duplicate-key tests pass.                                                                                  |
| v0.7 layer composition and post-merge hooks               | No current composer or hook callers were found; retain existing loader structure unless the API migration requires a mechanical call-site update.                                                       |
| v0.7 CLI defaults and localized parsing                   | No clap default attributes or localizer/catalogue source was found. Preserve current Clap/fallback diagnostics, exit codes, and streams through process tests under a controlled C locale.              |
| v0.9 discovery, YAML, aliases, localization, and metadata | Apply strict discovery and YAML contracts; use source injection for tests. Aliases, nested docs IR, agent context, metrics, and extra observability remain opt-in unless current code proves otherwise. |
| v0.9 and earlier cumulative changelog                     | The upstream v0.9.0 section explicitly covers v0.8.0 and earlier; review cumulative notes alongside the imported v0.6/v0.7 guides.                                                                      |

## Verification plan

No new algorithm or domain invariant is planned. Verification obligations are
integration contracts across OrthoConfig and mxd's CLI adapters:

- **OC-PRECEDENCE:** defaults < files < environment < explicit CLI, including
  a CLI value equal to the default and the selected subcommand namespace.
  Method: named/parameterized tests through the actual loader entry points.
  Domain: absent and explicit values at each used tier. Artefact: configuration
  tests under `src/server/cli.rs` and relevant `cli-defs` tests. Evidence:
  baseline red/green results plus the final SQLite and PostgreSQL test lanes.
- **OC-DISCOVERY:** no optional file, an explicitly selected missing file, a
  malformed file, and an unreadable file remain distinct and visible. Method:
  injected discovery/file sources for unit tests, plus child-process checks for
  observable CLI exit status and streams where available. Artefact: existing
  configuration test module and CLI integration tests. Evidence: exact
  diagnostic/status assertions and final feature-lane runs.
- **OC-CLI-OUTPUT:** configuration and clap parse errors retain their current
  fallback text and stream placement. The application has no locale fixtures,
  catalogue, or localized parse wrapper. Method: controlled child-process
  checks for help, unknown arguments, and malformed configuration, with a fixed
  locale. Artefact: CLI contract integration tests. Evidence: status, stdout,
  and stderr assertions; no translation behaviour is introduced.
- **OC-YAML:** YAML uses v0.10's supported provider and its YAML 1.2 semantics.
  Method: representative YAML fixtures including legacy booleans and duplicate
  keys. Artefact: config loader tests. Evidence: expected parse values/errors
  in SQLite, PostgreSQL, and wireframe-only feature selections as applicable.
- **OC-CLI-ARGS:** the subcommand-boundary scan passes through every token that
  belongs to the global options and removes exactly the subcommand and its
  arguments. Method: a `proptest` suite over generated option subsequences for
  the retention, stripping, `--`, and mixed cases, with the process-level
  contracts retained as the end-to-end specification. Artefact:
  `src/server/cli_tests/argument_scanning.rs`. Evidence: four generated
  properties and two named cases passing in the library suite.
- **OC-MANPAGES:** the build-time `cli-defs` consumer compiles and still emits
  its expected clap-mangen page. Method: root build/man-page target and
  snapshot or generated-output checks. Evidence: all required feature lanes
  plus focused man-page assertions.
- **OC-TEST-ISOLATION:** touched tests do not mutate process environment or use
  `figment::Jail`. Method: source search and injected `MapEnv` tests. Evidence:
  no new process mutation and no touched Jail helper remains.

The negative control for precedence is a lower-layer value that disagrees with
an explicitly supplied CLI default; the migrated test must fail if the CLI
layer is discarded. Discovery tests use distinct absent, malformed, and
permission-denied inputs so collapsing failures to defaults is observable. The
scanner does carry a general algorithm: it decides which prefix of an arbitrary
argument vector is a valid configuration prefix. Its invariant is therefore
stated over generated token sequences rather than exemplars. A formal proof is
still not proportionate, because the property is sampled over a bounded option
grammar whose acceptance decisions come from Clap itself, and a bounded
exhaustive check would only re-derive Clap's acceptance predicate.

## Plan of work

### EP-M0 — authoritative docs and baseline

Import and checksum the upstream guides, review the intervening changelog
history, inventory workspaces and consumers, and record baseline compiler,
feature lanes, tests, skips, and results. Go/no-go: docs hashes match the
pinned commit and baseline evidence identifies all direct and transitive
versions.

### EP-M1 — dependency and compiler alignment

Update both direct OrthoConfig declarations, preserve feature choices, remove
only redundant direct configuration dependencies, update the lockfile with
targeted Cargo operations, and align the advertised Rust floor and nightly pin
only when the actual graph requires it. Go/no-go: metadata and reverse trees
show one direct v0.10.0 line, explain any unavoidable older transitive copy,
and both runtime and build-time `cli-defs` targets compile.

### EP-M2 — runtime and test adaptation

Adapt source imports/providers and loader calls in `cli-defs` and runtime CLI
adapters. Replace `figment::Jail` tests with explicit `MapEnv` and file-source
inputs, without adding process-environment mutation or an application-owned
injection API. Add focused regressions for precedence, discovery failures,
localized diagnostics, exit status, and streams. Go/no-go: affected tests pass
in the PostgreSQL, SQLite, and wireframe-only lanes and generated man-page
output remains compatible.

### EP-M3 — integration, review, and pull request delivery

Run formatting, lint, Markdown, spelling, Mermaid, type-check, and test gates
in the repository's prescribed order. Review each coherent milestone with
CodeRabbit only after its deterministic gates pass; resolve every actionable
concern before continuing. Inspect final lockfiles, snapshots, generated docs,
and test-inventory deltas. Commit, push over SSH, set the requested upstream,
and create a draft PR referencing #669 and superseding #574. After the newly
raised acceptance gaps are closed and current-head checks and review reach
equilibrium, follow the requested PR-babysitting workflow and merge the PR.

## Milestones and plateaus

### EP-M0: imported documentation and baseline record

- Requirements advanced: issue #669 documentation-first and baseline clauses.
- Acceptance: pinned tag/commit match, five immutable copies have recorded
  hashes, relative links have an upstream mapping, and baseline gates are
  recorded.
- Conformance: no implementation edit preceded the guide import; scope and
  existing configuration boundary remain intact.
- Recovery: the document import is one atomic commit and can be reverted
  without source changes.
- Remaining: all dependency, implementation, and final-gate work.
- Compatibility decision: none.

### EP-M1: v0.10 dependency graph and compiler floor

- Requirements advanced: issue #669 dependency and toolchain clauses.
- Acceptance: both direct consumers resolve to published v0.10.0; package
  metadata records the chosen Rust floor; lockfile churn is targeted and
  explainable.
- Conformance: no new public application interface, format feature, or
  configuration precedence rule.
- Recovery: revert the isolated dependency/toolchain commit.
- Remaining: runtime loaders and behavioural adaptation.
- Compatibility decision: none.

### EP-M2: runtime consumers and regression coverage

- Requirements advanced: issue #669 source, test isolation, discovery, output,
  and behaviour clauses.
- Acceptance: existing configuration contracts pass across all required
  backend lanes; build-time man-page generation succeeds; no touched test uses
  `figment::Jail` or process environment mutation.
- Conformance: loading remains behind the shared CLI/configuration adapter;
  there is no new application injection port.
- Recovery: use the previous milestone commit as the dependency-only rollback
  point.
- Remaining: final gates, review, push, and PR.
- Compatibility decision: none.

### EP-M3: reviewed draft PR

- Requirements advanced: issue #669 delivery clauses.
- Acceptance: all applicable deterministic gates pass, CodeRabbit concerns are
  resolved, branch tracks the requested origin ref, and the draft PR includes
  `Closes #669`, `Closes #574`, and the Lody session link in terminal
  `## References`.
- Conformance: PR head matches the pushed branch; merge only after the
  acceptance gaps and current-head checks and review reach equilibrium. Do not
  publish a release.
- Recovery: no history rewrite is planned; use normal follow-up commits.
- Remaining: the four newly recorded acceptance gaps, fresh gates/review, and
  the authorized merge after equilibrium.
- Compatibility decision: none.

## Concrete steps

Run commands from the repository root. Keep Cargo gates sequential and use the
shared default cache. The repository's canonical commands are `make fmt`,
`make check-fmt`, `make lint`, and `make test`; Markdown validation is
`make markdownlint`, and Mermaid validation is `make nixie`. Capture long gate
output with `tee` under `/tmp` while preserving the command's exit status. Use
targeted `cargo update -p ortho_config --precise 0.10.0` only if Cargo's
lockfile does not already resolve the intended registry version.

Before source edits, preserve baseline counts and failures. After each
implementation milestone, inspect the full diff, update this plan, run its
focused gates and then the requested CodeRabbit review. Do not proceed through
a failed deterministic gate or unresolved actionable review finding.

## Validation and acceptance

- Baseline: record `rustc -Vv`, `cargo metadata`, all test targets and skip
  counts, and the result of `make test` at source commit `da9c7da3`.
- Dependencies: `cargo metadata --locked` and reverse `cargo tree -i` checks
  show the declared and resolved versions; no Git source for direct OrthoConfig
  remains.
- Precedence: tests exercise defaults, explicit file, environment, and explicit
  CLI values in increasing precedence, including the same-as-default CLI case.
- Errors: tests distinguish absent optional configuration, selected missing
  configuration, malformed YAML/TOML, unreadable files, invalid values, unknown
  subcommands, and localized parse failures, with status and stream assertions.
- Feature lanes: run the Makefile's PostgreSQL, SQLite, and wireframe-only
  targets in sequence, plus UI, verification, fuzz, concurrency, and doctest
  targets included by `make test`.
- Build-time consumer: build the root package and inspect man-page output so
  `cli-defs` compiles under the same published OrthoConfig API.
- Documentation: run `make fmt`, `make check-fmt`, `make markdownlint`, and
  `make nixie`; then recompute upstream-copy hashes and restore any imported
  bytes changed by formatters before committing.
- Final graph: inspect every maintained lockfile, `cargo tree -i` for each
  OrthoConfig version, Git sources, and dependency chains.
- Delivery: compare final test inventory to baseline and report every changed
  count, expected skip, unavailable gate, old dependency chain, and review
  concern.

### Baseline test inventory and results

The baseline commit was `da9c7da3da9b4f69a90fb7d38d405085f5a891a8`; all results
below predate source or manifest edits. Long command output was captured by the
baseline gate runner under `/tmp`.

| Command                    | Result                         | Inventory                                                                                                                 | Evidence                                                                                                                  |
| -------------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `make test`                | Failed at PostgreSQL bootstrap | 30 passed, 1 failed, 466 not run, 0 skipped of 497 across 33 binaries                                                     | `/tmp/test-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`                |
| `make test-sqlite`         | Failed after all cases ran     | 501 passed, 1 failed, 0 skipped of 502; one wireframe subprocess could not bind an address                                | `/tmp/test-sqlite-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`         |
| `make test-wireframe-only` | Passed                         | 500 passed, 0 skipped                                                                                                     | `/tmp/test-wireframe-only-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out` |
| `make test-rstest-bdd-ui`  | Timed out                      | One UI compile-contract test timed out at 300.009 seconds while compiling `pass_named_harness.rs`; no compiler diagnostic | `/tmp/test-rstest-bdd-ui-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`  |
| `make test-verification`   | Passed                         | 69 passed, 1 expected skip                                                                                                | `/tmp/test-verification-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`   |
| `make test-fuzz-harness`   | Passed                         | 8 passed, 0 skipped                                                                                                       | `/tmp/test-fuzz-harness-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`   |
| `make test-concurrency`    | Passed                         | 7 nextest tests and 7 doctests passed                                                                                     | `/tmp/test-concurrency-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`    |
| `make test-doc`            | Passed                         | 5 passed, 4 ignored                                                                                                       | `/tmp/test-doc-59318af2-3e23-4652-aeff-933ecec1b9dc-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`            |

The aggregate command stopped at its first PostgreSQL failure and did not run
the remaining targets, which were executed individually. The SQLite bind error
and UI timeout also occurred on the clean baseline. They remain visible for
final comparison and are not attributed to migration changes without evidence
connecting them to changed code.

### Final candidate validation

The corrected candidate passed the complete deterministic sequence. Compared
with the baseline, PostgreSQL and SQLite each gained 14 cases, and
wireframe-only gained 6. These net increases come from the new source-aware
configuration, format-provider, and process-level CLI contract tests; no
previous test suite was removed. Six CLI failure scenarios are now parameterized
`rstest` cases, so their generated names changed while their coverage remains
intact. The UI test that timed out on baseline passed 1/1.

| Gate                                 | Result                                         | Evidence                                                                                                                                                |
| ------------------------------------ | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `make check-fmt`                     | Passed                                         | `/tmp/check-fmt-59318af2-issue-669-current-6.out`                                                                                                       |
| `make test-workflow-contracts`       | Passed, 254 tests                              | `/tmp/test-workflow-contracts-59318af2-issue-669-current.out`                                                                                           |
| `make lint`                          | Passed, all configured feature lanes           | `/tmp/lint-59318af2-issue-669-current-4.out`                                                                                                            |
| `make typecheck`                     | Passed, PostgreSQL/SQLite/wireframe-only       | `/tmp/typecheck-59318af2-issue-669-current-2.out`                                                                                                       |
| `make rust-audit`                    | Passed; 7 allowed advisories reported          | `/tmp/rust-audit-59318af2-issue-669-current-2.out`                                                                                                      |
| `make warm-postgres` and alias check | Passed; cache and install contain `UTC -> UCT` | `/tmp/warm-postgres-59318af2-issue-669-current.out`                                                                                                     |
| `make test`                          | Passed across every listed suite               | `/tmp/test-59318af2-issue-669-current-4.out`                                                                                                            |
| Markdown lint, Nixie, and diff check | Passed                                         | `/tmp/markdownlint-59318af2-issue-669-current-2.out`, `/tmp/nixie-59318af2-issue-669-current-2.out`, `/tmp/diff-check-59318af2-issue-669-current-2.out` |
| CodeScene delta                      | Passed; health 9.38 to 10.00                   | `/tmp/codescene-delta-cli-contracts-59318af2-issue-669-current-2.out`                                                                                   |

The final aggregate inventory is PostgreSQL 511/511, SQLite 516/516,
wireframe-only 506/506, UI 1/1, verification 69 passed/1 expected skip, fuzz
8/8, concurrency 7/7 with 7 doctests, and documentation 5 passed/4 ignored.
Rust 1.94.0 `cargo check --locked --all-targets` passed for each CI feature
lane. Its compiler was `rustc 1.94.0 (4a4ef493e 2026-03-02)`. The repository
pin remains `nightly-2026-03-26`, resolving to
`rustc 1.96.0-nightly (80d0e4be6 2026-03-25)`. Whitaker used
`nightly-2026-05-28` (`rustc 1.98.0-nightly`). Identities are recorded in
`/tmp/rust-toolchain-identities-59318af2.out`.

Hosted RustSec findings affected versions already present on baseline. Targeted
lockfile updates moved `ctutils` 0.4.2 to 0.4.4, `rustls` 0.23.45 to 0.23.46,
and `rustls-webpki` 0.103.15 to 0.103.16. The final audit exits successfully
with seven configured allowed advisories reported; no audit warnings were
suppressed.

`make check-locked` passed. The locked reverse dependency tree shows v0.10.0
used by root `mxd` and `cli-defs`, including `cli-defs` in the root build
dependency. The only v0.5.0 copy is the `pg-embed-setup-unpriv 0.5.2`
development dependency chain. Tree output is in
`/tmp/cargo-tree-ortho-config-0.10.0-59318af2.out` and
`/tmp/cargo-tree-ortho-config-0.5.0-59318af2.out`.

Fresh PostgreSQL archives omitted the `UTC` alias while retaining `UCT`.
`warm-postgres` now repairs the relative alias in both its binary cache and
isolated install. The workflow contract test verifies both paths, and the full
PostgreSQL lane passed against that fresh setup.

### Final candidate validation (2026-10-30)

The gates were rerun sequentially against the acceptance-gap commits, with no
other check overlapping them.

| Gate                | Result                                   | Evidence                                                                                     |
| ------------------- | ---------------------------------------- | -------------------------------------------------------------------------------------------- |
| `make check-fmt`    | Passed; 68 files unchanged               | `/tmp/check-fmt-59318af2-*-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`        |
| `make typecheck`    | Passed; `GATE_RC=0` after 607s           | `/tmp/typecheck-59318af2-*-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.status`     |
| `make lint`         | Passed; `GATE_RC=0` after 871s           | `/tmp/lint-59318af2-*-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.status`          |
| `make markdownlint` | Passed; 0 issues in 68 files             | `/tmp/markdownlint-59318af2-*-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`     |
| `make nixie`        | Passed; all diagrams validated           | `/tmp/nixie-59318af2-*-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0.out`            |
| `make test`         | Passed; `MAKE_EXIT=0` across every suite | `/tmp/test-59318af2-*-issue-669-migrate-from-ortho-config-v0-3-0-to-v0-10-0-isolated-r2.out` |

The final inventory is PostgreSQL 527/527, SQLite 533/533, wireframe-only
515/515, the UI compile fixture 1/1, verification 69 passed with one expected
skip, fuzz 8/8, concurrency 7/7 plus seven doctests, and documentation five
passed with four ignored. The UI fixture that previously exhausted its
300-second limit passed in 189.9s, so that earlier timeout did not reproduce.

Two earlier aggregate runs failed in embedded-PostgreSQL bootstrap rather than
in any assertion: first `failed to connect to admin database: db error`, then
`postgresql_embedded::setup() failed`. A sibling mxd worktree was running the
same suite against the per-user default data directory
`/var/tmp/pg-embed-<uid>/data`. Giving this candidate its own
`PG_DATA_DIR=/var/tmp/mxd-issue-669-pgdata-59318af2-r2`, as the developers'
guide prescribed for concurrent checkouts under `pg-embed-setup-unpriv` 0.5.2,
made the lane pass. The failures are environmental and are recorded here rather
than suppressed.

That workaround is now historical. The rebase brought in
`pg-embed-setup-unpriv` 0.6.3, which gives each cluster its own data directory
and retries the residual slot-claim race itself, so the post-rebase run needed
no `PG_DATA_DIR` override and hit no `slot lock` contention. The current
guidance lives in the developers' guide's "Embedded PostgreSQL in tests"
section.

### Post-rebase validation (2026-10-30)

The rebase created a new candidate, so every gate was rerun from scratch at
`c2dadc8`, sequentially and with no overlapping run. Logs carry the `-rebase`
suffix so the earlier evidence keeps its own scope.

| Gate                           | Result                                         | Evidence                                    |
| ------------------------------ | ---------------------------------------------- | ------------------------------------------- |
| `make check-fmt`               | Passed; 70 files unchanged                     | `/tmp/check-fmt-*-rebase.out`               |
| `make typecheck`               | Passed; three feature lanes                    | `/tmp/typecheck-*-rebase.out`               |
| `make lint`                    | Passed; Clippy and Whitaker in three lanes     | `/tmp/lint-*-rebase.out`                    |
| `make test`                    | Passed; all eight lanes                        | `/tmp/test-*-rebase.out`                    |
| `make markdownlint`            | Passed; spelling current, 0 issues in 70 files | `/tmp/markdownlint-*-rebase.out`            |
| `make nixie`                   | Passed; all diagrams validated                 | `/tmp/nixie-*-rebase.out`                   |
| `make check-locked`            | Passed; `cargo metadata --locked` admitted     | `/tmp/check-locked-*-rebase.out`            |
| `make rust-audit`              | Passed; seven configured-allowed advisories    | `/tmp/rust-audit-*-rebase.out`              |
| `make test-workflow-contracts` | Passed; 266 cases plus ruff format and check   | `/tmp/test-workflow-contracts-*-rebase.out` |

The post-rebase inventory is PostgreSQL 529/529 (two cases inherited from the
new base), SQLite 533/533, wireframe-only 515/515, UI 1/1, verification 69
passed with one expected skip, fuzz 8/8, concurrency 7/7 plus seven doctests,
and documentation five passed with four ignored. The PostgreSQL lane hit no
slot contention because the new harness isolates cluster directories itself.

### Post-review repairs (2026-10-30)

CodeRabbit's assessment of `8b681fe` left three items. Each repair is recorded
here with its own evidence; the gate table for this head follows once the full
sequence has run.

- **Hosted Whitaker bootstrap.** All three `build-test` lanes failed in
  `install-whitaker` and took `build-test-result` with them. The cause is
  external to this diff: `dylint_driver 6.0.1` builds by cloning rust-clippy
  and checking out `master`, and upstream renamed that branch to `main`, so the
  pinned installer's cold bootstrap no longer resolves.
  `.github/workflows/ci.yml` now pins `install-whitaker` to `5a99aed`, which
  defaults to installer 0.2.11 and provisions a verified Clippy snapshot
  bundle. The previous head `4fd7a85` passed the same jobs on 2026-10-27 and
  `rust-toolchain.toml` is unchanged against `0c0b853d`, which is why the
  failure is not attributed to the migration.
- **`warm-postgres` UTC alias.** The alias loop skipped a pre-existing dangling
  `UTC` link, so `ln -s` failed with `File exists` and the failure was masked
  by the loop's last `find`. The loop now removes a dangling link before
  creating it and carries a sticky status through `exit`, so a failing root can
  no longer be hidden by a later one. A scratch tree with a dangling link, an
  already correct link, and a missing root reported `0`, `0`, and `1`
  respectively and left both aliases valid.
- **Scanner verification.** `config_args_without_subcommand` is now a thin
  wrapper over the pure `config_args_from`, with `subcommand_boundary`,
  `subcommand_names`, and `is_subcommand_name` extracted beside it.
  `src/server/cli_tests/argument_scanning.rs` states the invariant as four
  `proptest` properties over generated option subsequences plus two named
  cases, and all six passed in the library suite at the repaired head.
- **Lint findings in the new code.** Two Clippy errors surfaced only once the
  full lint lane ran, because `make lint` stops at its first failing
  prerequisite. `clippy::indexing-slicing` rejected `args[..end].to_vec()` in
  `config_args_from`; the slice is now
  `args.iter().take(end).cloned().collect()`, which cannot panic and needs no
  suppression. `clippy::doc_markdown` rejected an unbackticked `OrthoConfig` in
  the new test module's `//!` header; the sentence now names the parser the
  derive adds, with the crate name in backticks. Lesson: run `make lint` to
  completion locally before handing the ordered sequence to a gate runner, and
  freeze the tree while a sequence runs, because an edit mid-run leaves that
  gate's verdict describing a different tree.
- **Spelling gate and stale tool caches.** `make spelling` passed locally at
  the repaired tree while the hosted `docs-tooling` lane rejected an `-ise`
  spelling in the new test module's header, which en-GB-oxendict corrects to the
  `-ize` form. The local run reused a cached `uv` tool environment whose
  `typos` binary predated that correction. Reproducing the hosted command with
  fresh `UV_CACHE_DIR` and `UV_TOOL_DIR` failed on the same word, and the same
  command passed once it was corrected. Lesson: when a spelling result differs
  from the hosted lane, rerun the gate under fresh `uv` cache directories
  before trusting the local pass, and do not quote a rejected spelling in the
  plan, because the plan is scanned too.

## Idempotence and recovery

The upstream import can be repeated from the pinned commit and must reproduce
the recorded hashes. Cargo lockfile updates are deterministic when run through
Cargo. Before any push, verify branch identity, remote head, GitHub identity,
and exact candidate commit.

The branch was rebased once, on 2026-10-30, because `origin/main` advanced from
the recorded baseline `da9c7da3` to `0c0b853d` with the embedded-PostgreSQL
harness migration. The exclusive replay boundary is `da9c7da3`; all thirteen
branch commits descend from it, and the range contains no merges. Recovery refs
`refs/recovery/669-old-head`, `refs/recovery/669-old-base`,
`refs/recovery/669-target`, and `refs/recovery/669-remote-head` record the
pre-rebase head, boundary, target, and previously published remote head. Only
`Cargo.lock` conflicted; both times it was resolved by taking the target's file
wholesale and then re-resolving from the merged manifests with
`cargo metadata`, as the plan's packaging rule requires. `git range-diff` shows
eleven of the thirteen commits unchanged and the other two differing only in
the lockfile hunks that resolution replaced. A rewritten head needs a force
push with lease bounded to the recorded remote head, which the delivery step
authorizes.

## Artefacts and notes

- [Imported-document provenance and link map](../ortho-config-v0-10-0-provenance.md)
- Baseline and final gate logs under `/tmp`.
- Final dependency graph and test-inventory comparison, recorded here.
- Draft PR description with issue closure and Lody session reference.

## Interfaces and dependencies

Use the published `ortho_config = "0.10.0"` facade for runtime and derive
macros, with existing format features enabled on the runtime dependency. Use
`MapEnv`, `SharedEnvSource`, `SharedScanEnvSource`, and the release-provided
`*_with_sources` entry points only where they replace ambient environment in
existing configuration tests. Keep the existing clap parsing and man-page
composition in `cli-defs` and root `build.rs`; do not add `cargo-orthohelp`
unless inventory reveals an existing install or invocation that must move.

## Revision note

- (2026-10-26 12:25Z) Recorded the ten Jail-backed loader tests, the newest
  PostgreSQL helper's continuing `ortho_config ^0.5.0` requirement, and the
  build script's limited environment reads. This rules out an unnecessary
  helper update and narrows the remaining migration work.
- (2026-10-26 12:33Z) Confirmed the application has no Fluent catalogue,
  localizer, or localized parse wrapper. Reframed the output verification as
  stable fallback text, exit status, and stream checks through child processes.
- (2026-10-26 12:39Z) The initial baseline `make test` run stopped after 31 of
  497 tests because one PostgreSQL test could not connect to the admin
  database. No application or configuration source was changed; run the other
  feature lanes individually and keep the PostgreSQL failure visible until
  diagnosed.
- (2026-10-26 12:45Z) The standalone SQLite lane ran its complete 502-test
  inventory but one server subprocess failed to bind an address in use; the
  wireframe-only lane passed 500/500. The port failure happened on the
  unmodified baseline and remains separate from migration findings.
- (2026-10-26 13:02Z) The baseline inventory is complete: verification passed
  69 with one expected skip; fuzz passed 8; concurrency passed 7 tests and 7
  doctests; documentation passed 5 with 4 ignored. The UI compile-fail test
  timed out at 300 seconds under healthy but full compiler admission. No
  PostgreSQL or SQLite environment fix was established; both failures remain
  explicitly recorded against the clean baseline.
- (2026-10-26 13:07Z) The first documentation gate set passed after `make fmt`:
  `make check-fmt`, `make markdownlint` (which ran spelling), and `make nixie`.
  The imported guide copies remain byte-identical to the verified upstream
  commit. Review follows once the existing documentation catalogue is updated
  and its gates are rerun.
- (2026-10-26 13:14Z) The local CodeRabbit agent review completed with nine
  minor findings. Fixed its two local ExecPlan findings about spelling and
  sentence completeness. The remaining findings concern grammar, citation
  markers, pronouns, and relative links in byte-identical upstream copies;
  those copies remain unchanged, and the provenance record now gives explicit
  local and immutable-upstream targets for their links and citations.
- (2026-10-26 17:24Z) The follow-up CodeRabbit review reported one minor link
  concern: local Markdown resolves an imported `users-guide.md` link to mxd's
  guide. Preserved the upstream copies and expanded provenance with an
  anchor-by-anchor map to the local OrthoConfig guide, as allowed by the
  migration request. The copied Markdown still retains its upstream links.
- (2026-10-26 17:24Z) The link-map review cleared that concern and raised two
  additional documentation findings. Added the v0.10.0 profile-metadata wire
  format nuance to provenance, and scoped the style-guide voice exception to
  byte-for-byte imports with provenance records. The source copies remain
  unchanged.
- (2026-10-27 02:35Z) The full branch review cleared both source test-isolation
  findings and returned zero findings after the deterministic gates passed on
  the corrected candidate. The implementation is ready for push and draft PR
  creation.
- (2026-10-27 03:32Z) Hosted CI exposed a PostgreSQL timezone archive missing
  `UTC` while retaining `UCT`; adding the relative alias in a disposable cache
  made all 511 PostgreSQL tests pass when tests installed from that cache. A
  later exact warm-target run reused the already installed runtime and showed
  that the cache-only repair does not update that copy. The warm target now
  repairs both cache and install, with its workflow contract test checking both
  paths. Hosted CodeScene also found duplicated CLI failure-contract tests, so
  those scenarios are parameterized with `rstest`. The current RustSec
  advisories affect baseline lock versions; only `ctutils`, `rustls`, and
  `rustls-webpki` were updated to the reported fixed releases.
- (2026-10-27 03:55Z) The corrected warm target passed with a fresh cache and
  install. The complete `make test` aggregate passed across PostgreSQL 511,
  SQLite 516, wireframe-only 506, UI 1, verification 69 plus one expected skip,
  fuzz 8, concurrency 7 plus 7 doctests, and documentation 5 plus 4 ignored.
  Workflow contracts passed 254 tests. Lint, typecheck, Rust audit, CodeScene
  delta, Markdown/Nixie, and Rust 1.94.0 checks passed. CodeRabbit review and
  delivery remain.
- (2026-10-27 07:01Z) CodeRabbit's review of commit `9602a59` identified three
  assertion-diagnostic and provenance-link findings; commit `81b1037` resolves
  them. The full deterministic gate sequence passed again, including all three
  Rust 1.94.0 feature lanes and the five imported-guide checksums. Pushed
  `81b1037` to draft PR #670 with the requested title, issue references, and
  Lody session link. Loom, CodeQL, TLA+ and security checks are terminal;
  GitHub Actions run `40464471304` and Release Dry Run `40464471411` are
  active. The final CodeRabbit review was rate-limited, so its requested
  cooldown is running before retry.
- (2026-10-27 09:38Z) CodeRabbit's final-head review found that the mode-`000`
  unreadable-file test could fail under `CAP_DAC_OVERRIDE`. Added an access
  probe that skips only when the process can actually read the file, while
  retaining the `PermissionDenied` assertion otherwise. The first full rerun
  passed every gate except Whitaker's 400-line module limit: the added test made
  `cli_tests.rs` 403 lines. Moved the file-error cases into
  `cli_tests/file_errors.rs`; the exact-candidate full gate sequence is being
  rerun before commit and another review.
- (2026-10-27 10:05Z; historical) Commit `e0f7d17` contains the
  capability-aware unreadable-file test and the extracted file-error module.
  The full local sequence passed, including all test lanes, lint/Whitaker, Rust
  1.94 feature checks, and imported-guide hashes. Hosted CI, Loom, release
  artefacts, CodeQL, TLA+, CodeScene, and Gecko checks also passed. The final
  CodeRabbit attempt was rate-limited after the requested cooldown; review was
  pending at this point.
- (2026-10-28) Draft PR #670 is pushed at
  `4fd7a85e7aeeb33f126e14614a529953e9d3c9d9`; its final CodeRabbit review
  returned zero findings. The new acceptance gaps recorded above supersede the
  earlier completion assessment. CLI scanner process coverage, precedence and
  credential conflict tests, independent `cli-defs` feature validation, and
  YAML scalar cases remain to be completed. The PR description must be updated
  after those changes. No merge has yet occurred; merge remains authorized
  after the candidate reaches equilibrium.

- (2026-10-28) The scanner now validates the prefix with Clap before treating
  a token as a subcommand. The pre-fix process regression failed on two
  separated `--database create-user` cases; the migrated process tests pass.
  Four-tier conflicts cover database, migration timeout, and Argon2 time and
  parallelism. A disposable SQLite process test proves CLI password precedence,
  omission from serialization, restoration, and the stored Argon2 hash.
- Independent `cli-defs` checks passed for default, no-default, and all seven
  nonempty combinations of `toml`, `json5`, and `yaml`: each ran zero unit
  tests and reported two ignored doctests. Feature trees show OrthoConfig's
  `serde_json` and `toml` defaults enabled in every selection. In the separate
  application graph, JSON5 and YAML were absent before four disabled-format
  assertions passed; YAML-enabled checks passed nine cases, including strings
  `yes`, `no`, `on`, `off`, booleans `true` and `false`, and duplicate keys.
  The JSON5-enabled check passed one case.
- Current candidate gates passed formatting, lint, typecheck, locked-graph and
  audit checks, 254 workflow contracts, PostgreSQL 527/527, SQLite 533/533,
  wireframe-only 515/515, verification 69 passed and one skipped, fuzz 8/8,
  concurrency 7/7 plus seven doctests, and documentation five passed with four
  ignored. The UI compile-fixture test timed out twice at its 300-second limit
  while waiting on shared Cargo cache and compiler admission; aggregate
  `make test` therefore failed and remains unresolved. Markdown lint found one
  spelling error, corrected before its required rerun. Mermaid validation
  passed. Logs use `/tmp/*-mxd-issue669-acceptance-v5*`. Fresh review, PR
  reconciliation, and merge remain pending.
- (2026-10-30) The UI timeout did not reproduce: the UI compile fixture passed
  in 189.9s, and aggregate `make test` exited 0 once the embedded cluster had a
  private `PG_DATA_DIR`. Two earlier aggregate failures
  (`failed to connect to admin database: db error`, then
  `postgresql_embedded::setup() failed`) were environmental collisions with a
  sibling worktree, not assertion failures.
- (2026-10-30) `origin/main` advanced to `0c0b853d` with the
  `pg-embed-setup-unpriv` 0.6.3 harness migration, so the branch was rebased
  from its `da9c7da3` boundary onto that target. Only `Cargo.lock` conflicted;
  both conflicts were resolved with the target's file and the lock was then
  regenerated with `cargo metadata`, whose only delta from the target is the
  OrthoConfig v0.10.0 resolution. The replays are recorded at
  `969a3c2..c2dadc8`; range-diff shows eleven commits unchanged and the other
  two differing only in the replaced lockfile hunks. The rewritten head passed
  all nine local gates, including 266 workflow contracts and a PostgreSQL lane
  of 529/529 that needed no data-directory override.
- (2026-10-31) CodeRabbit's pre-merge table left one error and three warnings.
  The error named missing optional-discovery coverage: every earlier
  configuration test selected a file through `MXD_CONFIG_PATH`. Added
  `optional_discovery_applies_builtin_defaults`, which loads with only a
  discovery environment rooted in an empty temporary directory and asserts the
  compiled-in bind, database, and Argon2 defaults, plus an unset migration
  timeout. The Developer Documentation warning was answered by "The warm-up
  repairs the time-zone alias" in the developers' guide, which states the two
  scan roots, `find -H`, dangling-link replacement, and failure propagation,
  and names the workflow-contract test. The Out of Scope row stays in scope
  with evidence: the alias repair is tooling this PR's own `test-postgres` gate
  needs, recorded in the risk register and the 2026-10-27 entry, and covered by
  the executing contract test. The Compile-Time/Ui row remains an
  evidence-backed rebuttal, because the repository has no trybuild target and
  the derive behaviour is exercised through the runtime, process, and man-page
  contracts.
