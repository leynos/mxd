# rstest-bdd v0.6.0 migration evidence

Status: implemented and locally validated. Local notes are separate from the
immutable upstream copies. This document records the baseline, applicability,
decisions, and validation so the migration can be continued from the working
tree.

## Authoritative imports

The upstream repository is [leynos/rstest-bdd][upstream]. A clean-environment
Git fetch of `refs/tags/v0.6.0` resolved to
`72fb22635670e456545ca368805ba4c1c9d7bd69`, exactly the surveyed commit.
Imports were extracted with `git show COMMIT:PATH` before dependency or test
edits. Both guides were read before implementation.

| Original path                    | Destination                                 | SHA-256                                                            |
| -------------------------------- | ------------------------------------------- | ------------------------------------------------------------------ |
| `docs/users-guide.md`            | `docs/rstest-bdd-users-guide.md`            | `1e9f4d1b6607fdf83df979676f1a682218cd6751381d374cb98b701778b798b2` |
| `docs/v0-6-0-migration-guide.md` | `docs/rstest-bdd-v0-6-0-migration-guide.md` | `6e76c10028f9962134f6158732cf1645ae1b0beba3018bccd3bc3e88b2c38985` |

*Table 1: Byte-for-byte imports from the verified v0.6.0 commit.*

The adjacent [machine-readable link map](rstest-bdd-v0-6-0-link-map.json)
records all 13 relative targets in the imported guides. Markdown renderers do
not apply JSON mappings, so the following links make each preserved relative
target navigable without changing the upstream text. Every link points to the
same verified commit.

From `rstest-bdd-users-guide.md`:

- `v0-6-0-migration-guide.md#migrate-a-stateful-gpui-test` points to the
  [upstream migration guide][upstream-gpui-migration].

From `rstest-bdd-v0-6-0-migration-guide.md`:

- `adr-006-fallible-scenario-functions.md` points to
  [ADR 006][upstream-adr-006].
- `adr-007-harness-context-injection.md` points to [ADR 007][upstream-adr-007].
- `adr-009-consistent-implicit-fixture-name-normalization.md` points to
  [ADR 009][upstream-adr-009].
- `adr-012-guard-based-stepcontext-borrowing.md` points to
  [ADR 012][upstream-adr-012].
- `developers-guide.md` points to the
  [upstream developers' guide][upstream-developers].
- `rstest-bdd-design.md` and its `#2761`, `#2762`, `#2763`, and `#2765`
  anchors point to the [upstream design][upstream-design],
  [borrow-constraint section][upstream-design-2761],
  [interim GPUI section][upstream-design-2762],
  [v0.6.0 beta section][upstream-design-2763], and
  [v0.7.0 redesign section][upstream-design-2765], respectively.
- `users-guide.md#published-gpui-022-stateful-step-variants` and
  `users-guide.md#stateful-gpui-scenarios-with-durable-handles` point to the
  [published GPUI variants][upstream-users-variants] and
  [durable-handle guidance][upstream-users-handles], respectively.

Links to upstream `main` and `latest` remain part of the untouched text; they
are not migration authority. For rstest-bdd package API references, select
version `0.6.0` on docs.rs.

## Baseline and repository inventory

Baseline: `b52f971833d666a5a5769386e47ae03e6c680a42`. The starting branch was
`session/dca659dd`; the working tree was clean. No nested `AGENTS.md`,
submodule manifest, or independent maintained lockfile was found. There is one
workspace with seven manifests: root, `cli-defs`, `crates/mxd-concurrency`,
`crates/mxd-verification`, `fuzz`, `test-util`, and `validator`. All share the
root `Cargo.lock`. The root and verification crate consume the inherited BDD
dependencies; the other five packages do not directly consume BDD. Existing
historical v0.4/v0.5 plans and guides remain historical records.

Both inherited requirements and resolved packages start at `0.5.0`:
`rstest-bdd`, `rstest-bdd-macros`, `rstest-bdd-patterns`, and
`rstest-bdd-policy`. The macro dependency enables `compile-time-validation`;
its default features remain enabled. There are no BDD aliases, Git/path
replacements, patches, or strict-validation overrides. The workspace's
unrelated Diesel and configuration Git dependencies remain unchanged.

The baseline contains 145 step definitions in the root integration tests,
verification tests, and the inline `src/wireframe` and `src/server/wireframe`
test modules. It contains 82 Gherkin scenario declarations across the root and
verification suites. Runtime selection chooses one of two declarations through
`legacy-networking`. Baseline nextest inventories contain 65 executable feature
scenarios for PostgreSQL and 67 each for SQLite and wireframe-only. No scenario
outlines are present. The differences follow existing backend and networking
gates.

An existing open draft migration, [PR #615][existing-pr], was discovered before
creating a duplicate. Its implementation already handles the Tokio polling
boundary. GitHub renamed its head branch to `adopt-rstest-bdd-v0-6-0` through
the branch-rename API, which closed the original PR. Its migration commit
`69e313d6a8fdbe0db361c721f5a7a244da245991` is retained for a replacement draft
PR, with no duplicate open migration PR. The local branch tracks
`origin/adopt-rstest-bdd-v0-6-0`.

## Compiler policy

At the migration baseline commit, the repository pinned `nightly-2025-11-08`,
with rustfmt, Clippy, llvm-tools-preview, and rust-analyzer components. The
baseline and original migration candidate used
`rustc 1.93.0-nightly (843f8ce2e 2025-11-07)`, host `x86_64-unknown-linux-gnu`,
LLVM 21.1.3. There was no advertised `rust-version` or separate stable MSRV
policy. Cargo reported `1.93.0-nightly (636800288 2025-10-31)`; rustfmt
reported 1.8.0-nightly and Clippy 0.1.93 at the rustc commit.

The rebased PR base includes main commit
[`f19f250`](https://github.com/leynos/mxd/commit/f19f250edf9f8375db406efa60edf305680fbcef),
which moved the shared pin to `nightly-2026-03-26`. That base change followed
the PostgreSQL test-stack update to `postgresql_embedded`,
`postgresql_commands`, and `postgresql_archive` 0.20.4, whose declared floor is
Rust 1.94. Locked metadata for the rebased graph contains 588 packages; its
highest declared `rust_version` is 1.94.0 (`postgresql_archive` 0.20.4). All
six rstest-bdd 0.6.0 packages declare Rust 1.88. The repository still has no
root `rust-version` or separate stable MSRV policy.

The current pinned identity is `rustc 1.96.0-nightly (80d0e4be6 2026-03-25)`,
host `x86_64-unknown-linux-gnu`, LLVM 22.1.2; Cargo is
`1.96.0-nightly (e84cb639e 2026-03-21)`, rustfmt is 1.9.0-nightly, and Clippy
is 0.1.96 at the rustc commit. The required components are installed. This
nightly change is inherited from the rebased main branch, not introduced by the
rstest-bdd migration. No edition or Polonius policy changed.

At the original migration candidate, the locked PostgreSQL dependencies
`postgresql_embedded`, `postgresql_commands`, and `postgresql_archive` 0.20.2
declared Rust 1.92, and `cargo-platform` 0.3.3 declared Rust 1.91. Those floors
were supported by that candidate's pinned nightly. No separate stable floor is
claimed to have been tested.

The installed Whitaker driver independently selects `nightly-2026-05-28` for
Dylint. Its validated identity is
`rustc 1.98.0-nightly (57d06900f 2026-05-27)`, LLVM 22.1.6, with cargo-dylint
6.0.1. All three canonical Whitaker feature lanes pass with that existing tool
selection. No Polonius flag or borrow-checker policy changes.

## Applicability matrix

| Change                             | Evidence and applied action                                                                                                                                                       |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Underscore fixture normalization   | Existing remaps remain exact; three regression cases verify implicit normalization, one underscore removal, and explicit preservation.                                            |
| Aliased step errors and payloads   | `CommandResult` is stored as workflow state, not a step return. Consumer regressions verify error propagation and successful typed overrides.                                     |
| Scenario results/fallible fixtures | Existing scenario bodies return unit and fixtures provide owned worlds. No payload or borrowed-result migration is needed.                                                        |
| Tokio runtime selector             | Five consumers migrated: two ready-step suites select canonical TokioHarness; three suspending suites use external Tokio tests.                                                   |
| Custom adapters/context/GPUI       | No existing implementation or consumer; design proposals do not authorize new harnesses.                                                                                          |
| Direct runtime methods             | No direct caller at baseline; new regression tests handle the must-use `InsertOutcome` with `is_inserted()`. No bypass-reporting consumer.                                        |
| Reporting paths                    | No owned JSON/JUnit snapshots or cargo-bdd consumers. Verification binds a root feature outside its crate manifest; that path remains absolute. No snapshot blessing is required. |
| Language-server APIs               | No dependency or owned integration; no discovery/indexing migration.                                                                                                              |
| Feature rebuild inputs             | All bound files share the manifest filesystem root. No hermetic build-input manifest needs changing.                                                                              |
| Older release migration            | Starting version is 0.5.0, so no intervening pre-0.5 migration is needed.                                                                                                         |

*Table 2: Relevant breaking changes and demonstrated inapplicable surfaces.*

## Dependency alignment

The root workspace requirements change from `"0.5.0"` to `"0.6.0"` for runtime
and macros, preserving `compile-time-validation` and default features. Both
existing consumers inherit these requirements. The root test dependency adds
only `rstest-bdd-harness-tokio = "0.6.0"` for its existing ready-step suites.
The base harness is transitive; no GPUI shim or upstream path dependency is
imported.

The single lockfile resolves runtime, macros, patterns, policy, harness, and
Tokio harness to final `0.6.0`, with no old BDD package or prerelease. The
retained migration commit supplies the Cargo-generated lockfile. A targeted
`cargo update -p rstest-bdd --precise 0.6.0` confirmed that version but
reselected five unrelated Windows dependency edges; restoring the retained
lockfile avoids that unnecessary churn. Locked metadata validates the graph.

Necessary BDD transitive changes include Gherkin 0.14 to 0.16, ctor 0.2 to 1.0,
typed-builder 0.15 to 0.23, derive_more 2.0 to 2.1, proc-macro-error3, and
cargo_metadata 0.23. Redundant cap-std/cap-primitives 3.x and older macro
support disappear; existing 4.x capability dependencies remain. Unrelated
package versions and Git revisions are unchanged.

## Documentation and API discrepancies

The imported text labels the harness-context marker as v0.6.1 beta and
concurrent fixture guards/lifecycle guidance as v0.7.0. These labels are not
instructions to adopt later-release APIs. The published runtime and macro
packages' `.cargo_vcs_info.json` both identify the verified commit. Inspection
of that immutable source and the registry packages shows `borrow_mut(&self)`,
`try_borrow_mut`, and macro marker classification already present. Existing
consumers do not need those APIs; retain `#[from(...)]` spelling and existing
scenario isolation without assuming additional lifecycle guarantees.

The user guide says async steps are unsupported under TokioHarness, whereas its
migration guide and published wrapper implementation permit a single immediate
poll. Published `rstest-bdd-macros/src/codegen/wrapper/emit/mod.rs` explicitly
fails on `Poll::Pending` with the harness-runtime error. Suspending steps must
use an async scenario under an external current-thread Tokio test attribute.

## Validation and progress

- 2026-10-02: skills loaded, clean baseline inspected, existing PR discovered,
  tag verified, guides imported/read, and relative-link map recorded.
- Baseline `make test`: passed at the baseline commit. PostgreSQL 488/488,
  SQLite 493/493, wireframe-only 491/491, verification 69 passed with one
  existing TLC skip, concurrency 7/7. SQLite doctests: 5 passed, 4 existing
  ignores. Concurrency doctests: 7 passed. Log: `/tmp/mxd-baseline-test.out`.
- Baseline `make check-locked` passed. Metadata contains 591 packages and
  seven workspace members. Three nextest JSON inventories were captured in
  `/tmp/mxd-baseline-nextest-{postgres,sqlite,wireframe-only}.json` before
  edits.
- The baseline alias regression compiled, then failed at its intended
  assertion (exit 101): 0.5.0 boxed an aliased `Err` as a payload. Log:
  `/tmp/mxd-alias-baseline-red.out`. The migrated suite keeps that contract.
- Retained the existing migration commit and replaced positional async
  bindings with checked feature names. Extracted transaction-encoding scenario
  bindings to keep each Rust file below 400 lines.
- Migrated focused tests: 9 passed (6 consumer regressions and 3 binding
  coverage checks). Formatting, locked metadata, and all three typecheck
  configurations passed. Full lint/test and final inventory comparisons also
  passed; all original scenario bindings are accounted for.

| Root feature lane                                  | Baseline | Migrated | Skips |
| -------------------------------------------------- | -------- | -------- | ----- |
| PostgreSQL + legacy networking + test support      | 488      | 497      | 0     |
| SQLite + default features + test support           | 493      | 502      | 0     |
| SQLite + TOML + test support, no legacy networking | 491      | 500      | 0     |

*Table 3: Passed root test lanes; each adds the same nine regressions.*

The additions are two alias-result tests, three fixture-key cases, one
suspending scenario, and three feature-binding coverage cases. Existing feature
files are unchanged. The new feature adds one scenario declaration (82 to 83)
and six step definitions (145 to 151). Fifteen existing async scenarios retain
their feature names and step bodies but acquire explicit Rust test names; ten
bindings move into the encoding test's child module.

The complete sequential validation passed:

- `make fmt`, `make check-fmt`, `make check-locked`, `make typecheck`, and
  `make lint`, including all three Clippy and Whitaker configurations.
- `make test`: the root lanes above; verification 69 passed with the existing
  Docker-required TLC skip; concurrency 7 passed and 7 doctests passed; SQLite
  workspace doctests 5 passed with 4 existing ignored examples.
- `make markdownlint`, `make nixie`, `make test-workflow-contracts` (172),
  `make test-spelling-gate` (14), `make test-codescene-boundary` (83), and
  `make test-dependabot-policy` (18).
- `make check-loom`, `make test-loom` (all 5 expected models, maximum
  preemptions 3), and `make test-loom-runner` (18).
- `make test-validator-sqlite` and `make test-validator-postgres`: 37 tests
  each, with `MXD_VALIDATOR_FAIL_CLOSED=true` and SynHX release 0.1.48.1
  installed by the repository script with its published checksum verification.
- `make audit`: passed with the existing warnings detailed below.

Initial formatting checks used CI's trusted mdtablefix 0.6.0 binary. The
post-turn hook uses the host's 0.6.1 version and exposed three existing
formatting differences in owned historical/design documents. A follow-up
repairs their Markdown structure and wrapping so both versions accept the same
text. The immutable guides remain excluded and checksum-verified. Every logged
pipeline preserves the command's exit status. Logs reside at
`/tmp/mxd-migration-<target>.out`; baseline evidence is at
`/tmp/mxd-baseline-test.out`. Compiler identities, final metadata, and the
three nextest inventories are stored under `/tmp/mxd-migration-*`.

The nextest ID comparison maps every removed generated ID to its explicit
replacement: 13 for PostgreSQL and 15 for each SQLite lane. The only unmatched
additions are the nine new tests above. No existing scenario or step body is
removed. No stale BDD version, prerelease, alias, or patch remains in the final
workspace graph or lockfile. No old BDD dependency chain remains.

The dependency audit passes with four existing allowed warnings: bincode 2.0.1
(`RUSTSEC-2025-0141`, direct and via wireframe), paste 1.0.15
(`RUSTSEC-2024-0436`, direct), event-listener 5.4.1 (`RUSTSEC-2026-0221`, via
sqlx-core and the PostgreSQL test stack), and yanked chacha20 0.10.1 (via rand
0.10.3 and PostgreSQL dependencies). Those versions already occur in the
baseline lockfile and are unchanged. No audit suppression is introduced. The
audit scans the complete root workspace lockfile; its six skipped manifests
have no independent adjacent lockfile.

### Rebased candidate validation (2026-10-11)

The current main base is `65eb8ca39214dbbc4ef4651c5b7ee6c8f2d23542`. All 17
assigned local targets passed sequentially on the rebased candidate:
`make check-fmt`, `make lint`, `make typecheck`, `make test`,
`make -o spelling markdownlint`, `make nixie`, `make check-locked`,
`make test-workflow-contracts`, `make test-spelling-gate`,
`make test-codescene-boundary`, `make test-dependabot-policy`, `make audit`,
`make check-loom`, `make test-loom`, `make test-loom-runner`,
`make test-validator-sqlite`, and `make test-validator-postgres`. Gate logs and
durations are recorded in
`/tmp/gate-results-dca659dd-c303-4df0-b113-2d19de1540c9-adopt-rstest-bdd-v0-6-0.tsv`.

The three root Nextest feature lanes passed 497, 502, and 500 tests, with no
skips. Verification passed 69 tests with the existing Docker-required TLC
scenario skipped; fuzz passed 8, concurrency passed 7, and concurrency doctests
passed 7. SQLite doctests passed 5 with 4 existing ignored examples. The
feature inventory remains 18 files and 83 declarations, versus 17 files and 82
declarations at the rebased main base: the sole addition is
`Yielding steps retain fixture overrides`. The migration's nine regression
tests remain included. Validator SQLite and PostgreSQL gates both passed, as
did all three Clippy and configured typecheck lanes.

Cargo refreshed the lock metadata during lint/typecheck. The update selects
`thiserror` 2.0.21 for the graph, including the rstest-bdd 0.6.0 runtime,
macros, harness, patterns, and policy packages, and records dependencies for
`hashbrown` 0.16.0. `make check-locked` and the full test and audit targets
passed with that lockfile. The duplicate `rstest` 0.26.1 is only a dependency of
`pg-embed-setup-unpriv` 0.5.2; the workspace's direct `rstest` remains 0.27.0.
No duplicate rstest-bdd runtime or macro versions remain. The audit reported
the same four allowed warnings listed above. The managed sccache service was
unreachable during compilation, so the build-limits wrapper compiled directly;
compiler admission remained available and no gate failed.

## Delivery boundaries

The table formatter, Markdown linter, and local spelling policy exclude exactly
the two immutable imports. Their upstream prose is never rewritten. The
spelling gate regenerates `typos.toml`, including the current estate rules;
generated output is retained rather than hand-edited. Checksums remain
unchanged. BDD remains a test dependency; domain and application adapters are
unchanged.

The original candidate had no local migration blocker. TLC was not run
separately because Docker is unavailable; its existing ignored integration test
remains visible. The rebased candidate's configured local gates passed; hosted
CI and current-head review remain separate evidence. The requested tracking
branch is `adopt-rstest-bdd-v0-6-0`. The user authorized a squash merge once
the exact PR head reaches review and CI equilibrium; no release publication is
authorized.

[upstream]: https://github.com/leynos/rstest-bdd/tree/72fb22635670e456545ca368805ba4c1c9d7bd69
[existing-pr]: https://github.com/leynos/mxd/pull/615
[upstream-gpui-migration]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/v0-6-0-migration-guide.md#migrate-a-stateful-gpui-test
[upstream-adr-006]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/adr-006-fallible-scenario-functions.md
[upstream-adr-007]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/adr-007-harness-context-injection.md
[upstream-adr-009]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/adr-009-consistent-implicit-fixture-name-normalization.md
[upstream-adr-012]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/adr-012-guard-based-stepcontext-borrowing.md
[upstream-developers]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/developers-guide.md
[upstream-design]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/rstest-bdd-design.md
[upstream-design-2761]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/rstest-bdd-design.md#2761-borrow-constraint-exposed-by-gpui-adoption
[upstream-design-2762]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/rstest-bdd-design.md#2762-interim-gpui-state-pattern
[upstream-design-2763]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/rstest-bdd-design.md#2763-v060-beta2-quick-wins
[upstream-design-2765]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/rstest-bdd-design.md#2765-v070-pre-100-redesign
[upstream-users-variants]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/users-guide.md#published-gpui-022-stateful-step-variants
[upstream-users-handles]: https://github.com/leynos/rstest-bdd/blob/72fb22635670e456545ca368805ba4c1c9d7bd69/docs/users-guide.md#stateful-gpui-scenarios-with-durable-handles
