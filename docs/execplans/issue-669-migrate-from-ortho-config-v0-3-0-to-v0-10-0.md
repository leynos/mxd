# Migrate mxd to OrthoConfig v0.10.0

This ExecPlan is a living document for issue
[#669](https://github.com/leynos/mxd/issues/669).

Status: IN PROGRESS

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
  configuration tests, CI/toolchain metadata, and migration documentation
  trigger a scope review before continuing.
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

- The lockfile includes OrthoConfig 0.5.0 through the PostgreSQL test helper.
  Severity: medium; likelihood: high. Crates.io lists helper v0.6.3 as the
  latest release, and its dependency API still requires `ortho_config ^0.5.0`.
  Keep the existing helper rather than taking unrelated dependency churn that
  would not remove the duplicate.
- YAML now uses the stricter Saphyr provider. Severity: medium; likelihood:
  medium. Exercise existing YAML fixtures and YAML 1.2 edge cases rather than
  assuming the parser swap is transparent.
- The repository advertises a Rust floor that may be below v0.10.0's 1.89.0
  floor. Severity: medium; likelihood: medium. Inspect all promised feature
  lanes and compiler-coupled tooling, then update the floor only as required.
- Global Markdown formatting can alter imported upstream copies. Severity:
  low; likelihood: medium. Restore those files from the pinned commit after
  formatting and verify the recorded SHA-256 values.
- Build and test work shares Cargo's cache and compiler admission with other
  repositories. Severity: low; likelihood: medium. Use the shared cache and
  allow healthy queues to make progress; do not bypass admission.

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
  test-helper dependency chain.
- [x] (2026-10-26 12:25Z) Confirmed that the latest published PostgreSQL test
  helper (v0.6.3) still requires `ortho_config ^0.5.0`; the 0.5.0 lockfile
  entry remains an explained transitive dependency.
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
- [ ] Complete the v0.10.0 applicability matrix with source-level evidence.
- [ ] Migrate manifests, lockfile, compiler floor, source imports, loaders, and
  isolated configuration tests.
- [ ] Run behavioral and feature-lane validation, generated-man-page checks,
  and final deterministic gates.
- [ ] Complete milestone CodeRabbit reviews, push the branch, and create a
  draft pull request that closes #669 and supersedes #574.

## Surprises & discoveries

- Observation: the root lockfile resolves OrthoConfig 0.5.0 as well as the
  directly declared Git-tagged 0.3.0 release. Evidence:
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
- Observation: the newest published `pg-embed-setup-unpriv` release still
  depends on the old OrthoConfig line. Evidence: Firecrawl retrieved crates.io
  metadata for v0.6.3 and its dependency list declares `ortho_config ^0.5.0`,
  the same range as v0.5.2. Impact: do not update this test helper merely to
  chase a single OrthoConfig version; retain and explain the dependency chain.
- Observation: `build.rs` only reads Cargo's `OUT_DIR` and `CARGO_PKG_NAME`;
  no `cargo-orthohelp` bridge is installed or invoked by this repository.
  Evidence: `build.rs` has the only application build-time environment reads,
  and repository search found no `cargo-orthohelp` command or metadata table.
  Impact: keep the existing clap-mangen generator and validate its actual
  output under the v0.10 build-time dependency.

## Decision log

- Decision: import the five upstream files from the verified tag's immutable
  commit and preserve their bytes. Rationale: issue #669 requires version-bound
  authoritative material and forbids substituting mutable documentation
  sources. Date/Author: 2026-10-26, Codex.
- Decision: keep OrthoConfig at the existing configuration adapter boundary and
  use its v0.10.0 source-aware APIs for hermetic tests. Rationale: this replaces
  `figment::Jail` without adding an application-owned injection API or
  changing domain dependencies. Date/Author: 2026-10-26, Codex.
- Decision: remove direct configuration crates only when every remaining
  production and test import is supplied by the v0.10.0 facade or no longer
  needed. Rationale: the older guides document re-exports, but mxd's own
  imports decide whether a direct dependency remains necessary. Date/Author:
  2026-10-26, Codex.
- Decision: keep `pg-embed-setup-unpriv` at 0.5.2 unless the migration itself
  requires an API update. Rationale: crates.io's latest 0.6.3 release still
  depends on `ortho_config ^0.5.0`, so changing this helper would add unrelated
  churn and would not remove the locked 0.5.0 copy. Date/Author: 2026-10-26,
  Codex.

## Outcomes & retrospective

Implementation and validation are in progress. The baseline aggregate test
command is red for embedded PostgreSQL initialization and stopped before the
remaining lanes, which were run individually and are listed above. Record the
final dependency graph, test inventory delta, compiler identities, review
findings, and pull request URL here before marking the plan complete.

## Context and orientation

The workspace root and `cli-defs/Cargo.toml` both pin `ortho_config` to Git tag
`v0.3.0`. `cli-defs/src/lib.rs` defines `AppConfig` and shared CLI structs; the
crate is used both by runtime binaries and the root `build.rs` man-page
generator. `src/server/cli.rs` calls `AppConfig::load_from_iter`, while
`src/server/admin.rs` uses the subcommand merge API. Existing configuration
tests in `src/server/cli.rs` use `figment::Jail`, environment mutation, and
temporary config files.

The root lockfile also contains registry OrthoConfig 0.5.0 through
`pg-embed-setup-unpriv 0.5.2`. The baseline compiler is
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

| Guide change                                                       | Applicability and planned evidence                                                                                                                                                                 |
| ------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Update OrthoConfig, direct macro, and tooling versions together    | Required for root and `cli-defs`; inventory direct macro and tooling use, then inspect manifests and lock graph.                                                                                   |
| Explicit CLI values win even when equal to defaults                | Review both root `load_from_iter` and subcommand merge paths; add a regression for explicit default versus lower file/environment values.                                                          |
| Fluent identifiers and docs IR 2.0                                 | No current Fluent catalogue, localizer, `LocalizedParse` call, or IR-version consumer was found; confirm by final source search. The CLI currently uses Clap and OrthoConfig fallback diagnostics. |
| `AgentContext.profiles` uses `ProfilesDeclaration`                 | No current `AgentContext` construction was found; confirm final source search.                                                                                                                     |
| `DocMetadata` literals add `profiles`                              | No library or bridge `DocMetadata` literals were found; `cargo-orthohelp` is not an installed or invoked consumer.                                                                                 |
| Boolean flags accept `--flag=false`                                | No boolean field exists in shared CLI configuration; confirm source inventory and generated man pages. The space-separated form remains rejected by v0.10.0.                                       |
| Named profiles                                                     | Optional and unused; do not reserve `profile` or alter the config schema.                                                                                                                          |
| Stacked discovery scopes                                           | Optional; retain current first-wins discovery behavior.                                                                                                                                            |
| Injected environment and file sources                              | Applicable to replace `Jail` and process-state tests; use only published `_with_sources` entry points and `MapEnv`.                                                                                |
| Inferred string defaults with `cli_default_as_absent`              | No clap `default_value`/`default_value_t` fields were found; do not add the attribute without a demonstrated need.                                                                                 |
| Agent-native metadata and policy checking                          | Optional; no manifest policy tables or `cargo-orthohelp` invocation exists.                                                                                                                        |
| Cargo external-subcommand helper                                   | Optional and unrelated to mxd's server CLI; do not adopt.                                                                                                                                          |
| Standalone `ortho_l10n`                                            | No standalone localization-only consumer exists; retain localization through the facade if needed.                                                                                                 |
| `cargo-orthohelp` stdout changes and bridge environment allow-list | No bridge invocation exists. Review `build.rs`, which currently reads only Cargo-provided `OUT_DIR` and `CARGO_PKG_NAME`, and validate clap-mangen output.                                         |
| Localization re-exports, including `FluentValue`                   | No localization runtime or `fluent-bundle` dependency is used by this repository; do not add `ortho_l10n` or localization identifiers.                                                             |
| Missing, malformed, and unreadable config remain distinct          | Required behavior; add or retain tests proving each outcome, visible diagnostics, exit status, and streams.                                                                                        |

### Earlier migration-guide applicability matrix

| Earlier change                                            | Applicability and planned evidence                                                                                                                                                                      |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.6 dependency re-exports                                | Replace direct Figment, JSON5, YAML, `uncased`, or `xdg` dependencies only after source imports are moved to v0.10 re-exports and tests pass.                                                           |
| v0.6 declarative discovery and strict failures            | Audit manual discovery and missing/malformed/unreadable cases; keep discovery settings explicit and failures visible.                                                                                   |
| v0.6 Saphyr YAML 1.2 parsing                              | Replace direct `serde_yaml`/legacy YAML providers where used; test existing YAML plus YAML 1.2 and duplicate-key behavior.                                                                              |
| v0.7 layer composition and post-merge hooks               | No current composer or hook callers were found; retain existing loader structure unless the API migration requires a mechanical call-site update.                                                       |
| v0.7 CLI defaults and localized parsing                   | No clap default attributes or localizer/catalogue source was found; preserve current help/error exits and localized output through binary tests.                                                        |
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
permission-denied inputs so collapsing failures to defaults is observable. No
property test or formal proof is proportionate because the migration adds no
new state machine or general algorithm.

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

### EP-M3 — integration, review, and draft PR

Run formatting, lint, Markdown, spelling, Mermaid, type-check, and test gates
in the repository's prescribed order. Review each coherent milestone with
CodeRabbit only after its deterministic gates pass; resolve every actionable
concern before continuing. Inspect final lockfiles, snapshots, generated docs,
and test-inventory deltas. Commit, push over SSH, set the requested upstream,
and create a draft PR referencing #669 and superseding #574. Do not merge.

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
- Remaining: runtime loaders and behavioral adaptation.
- Compatibility decision: none.

### EP-M2: runtime consumers and regression coverage

- Requirements advanced: issue #669 source, test isolation, discovery, output,
  and behavior clauses.
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
- Conformance: PR head matches the pushed branch; no merge or release occurs.
- Recovery: no history rewrite is planned; use normal follow-up commits.
- Remaining: review feedback on the draft PR, if any.
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

## Idempotence and recovery

The upstream import can be repeated from the pinned commit and must reproduce
the recorded hashes. Cargo lockfile updates are deterministic when run through
Cargo. Before any push, verify branch identity, remote head, GitHub identity,
and exact candidate commit. The task branch starts at the recorded baseline;
there is no rebase or force-push in the plan.

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
