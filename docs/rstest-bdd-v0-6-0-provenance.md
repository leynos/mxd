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

Relative links are resolved by the adjacent
[rstest-bdd-v0-6-0-link-map.json](rstest-bdd-v0-6-0-link-map.json), which maps
original targets to that immutable upstream tree. Links to upstream `main` and
`latest` remain part of the untouched text; they are not migration authority.
For rstest-bdd package API references, select version `0.6.0` on docs.rs.

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

The repository retains `nightly-2025-11-08`, with rustfmt, Clippy,
llvm-tools-preview, and rust-analyzer components. The baseline and migrated
identity is `rustc 1.93.0-nightly (843f8ce2e 2025-11-07)`, host
`x86_64-unknown-linux-gnu`, LLVM 21.1.3. There is no advertised `rust-version`
or separate stable MSRV policy. Cargo reports
`1.93.0-nightly (636800288 2025-10-31)`; rustfmt reports 1.8.0-nightly and
Clippy 0.1.93 at the rustc commit. These tools all pass the canonical migration
gates.

Locked baseline and final metadata declare Rust 1.92 for `postgresql_embedded`,
`postgresql_commands`, and `postgresql_archive` 0.20.2 in the PostgreSQL test
graph. New `cargo-platform` 0.3.3 declares Rust 1.91. Both exceed upstream's
1.88 floor but remain supported by the existing nightly. No compiler pin or
component change is needed; no separate stable floor is claimed to have been
tested.

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

## Delivery boundaries

The table formatter, Markdown linter, and local spelling policy exclude exactly
the two immutable imports. Their upstream prose is never rewritten. The
spelling gate regenerates `typos.toml`, including the current estate rules;
generated output is retained rather than hand-edited. Checksums remain
unchanged. BDD remains a test dependency; domain and application adapters are
unchanged.

There is no remaining local migration blocker. TLC was not run separately
because Docker is unavailable; its existing ignored integration test remains
visible. The configured local gates passed; hosted CI remains separate evidence
from these results. Delivery is a gated commit and replacement draft PR on the
requested tracking branch. No release publication or merge is authorized.

[upstream]: https://github.com/leynos/rstest-bdd/tree/72fb22635670e456545ca368805ba4c1c9d7bd69
[existing-pr]: https://github.com/leynos/mxd/pull/615
