# Migrate MXD configuration to OrthoConfig v0.9.0

Status: IN PROGRESS

## Purpose / big picture

Upgrade the complete MXD-owned configuration path from the Git tag v0.3.0 to
published OrthoConfig 0.9.0. Both server binaries and build-time man-page
consumers must share the same CLI definitions and preserve defaults < file <
environment < explicit CLI precedence. Configuration failures must precede
network binding and database work.

## Constraints

- Preserve positional `create-user` credentials, password serialization
  exclusion, and the deliberate explicit CLI password restoration.
- Preserve validator injected-environment APIs. Issue #509 owns global
  environment enforcement and unrelated test-util mutation removal.
- Use #507's PostgreSQL, SQLite, and wireframe-only Make/CI lanes; never
  enable conflicting database backends together.
- Keep build.rs limited to command metadata and man-page rendering.
- Update only dependencies required by this migration, using the shared Cargo
  cache. Do not refresh unrelated dependencies or disrupt other agents.
- Keep domain and transport responsibilities unchanged. Configuration remains
  an inbound adapter at the executable composition boundary.

## Tolerances (exception triggers)

Escalate if the published release is unavailable, if satisfying its compiler
contract requires unrelated dependency repair, or if preserving existing CLI or
credential semantics requires a product decision. Record infrastructure and
baseline gate failures separately; do not treat them as passing evidence. No
arbitrary time or file-count limit applies to the authorized migration.

## Risks

- Generated helper and merge APIs may have changed since v0.3.0. Inspect the
  shipped implementation and exercise explicit values equal to defaults.
- Host build dependencies can retain format defaults and mask missing target
  edges. Test cli-defs independently and inspect both feature graphs.
- The release declares Rust 1.89.0, but the locked graph may require newer
  compilers. Inspect metadata before declaring the application minimum.
- Generated discovery reads configuration values from the process environment;
  MapEnv injects only discovery. Use #509's child-process design for real
  generated-loader tests, never parent-process mutation.

## Progress

- [x] (2026-10-02) Read repository guidance and requested skills; rename branch
  and session; inspect manifests and canonical Makefile feature lanes.
- [x] (2026-10-02) Read tagged v0.9.0, v0.6.0, and v0.7.0 guides with
  Firecrawl, including stricter discovery, YAML, re-exports, and merge APIs.
- [x] (2026-10-02) Record baseline gates and normal/build/development feature
  graphs; PostgreSQL tests have a pre-existing embedded-cluster failure.
- [x] (2026-10-02) Demonstrate the argv regression against the original loader
  and measure the published no-TOML compilation failure.
- [x] (2026-10-02) Implement the v0.9.0 dependency/features, compiler floors,
  parsed-CLI resolver, and configuration regression coverage.
- [x] (2026-10-02) Add `make test-cli-defs` and `make check-config-msrv`, and
  wire their conditional CI calls.
- [ ] M1: run canonical code and documentation gates, commit, and clear
  `coderabbit review --agent` concerns. Deterministic gates are complete;
  migration committed as b5d6886; one documentation review concern is being
  addressed before completing M1.
- [x] (2026-10-02) Complete isolated format/build-time/compiler evidence and
  current configuration documentation; include these checks in M1 review.
- [ ] M2: record review dispositions and publication evidence; gate and commit
  any required follow-up changes, then clear CodeRabbit concerns.
- [ ] Push and create a draft PR closing #574 with the Lody session reference.

## Surprises & discoveries

- `docs/contents.md` is absent in this checkout; repository-layout.md and the
  current user/developer guides provide orientation instead.
- The Makefile already implements separate backend lint/typecheck/test lanes.
  The older all-features instruction does not describe its current behaviour.
- The tagged index lists v0.6.0, v0.7.0, and v0.9.0 migration guides. Earlier
  guessed v0.4.0/v0.5.0 guide URLs return 404; use shipped code as authority.
- #509 is still open. No complete migration of test-util's global guards is
  folded into this task.
- Published v0.9.0 unconditionally imports its optional TOML provider. A
  standalone no-default compilation fails without TOML. The workspace
  dependency therefore keeps the TOML provider enabled even when a consumer
  disables its own default features; JSON5 and YAML remain opt-in.
- Discovery distinguishes selectors: `--config-path` is required, while
  `MXD_CONFIG_PATH` participates in optional discovery and may fall through to
  a later successful candidate. If no candidate succeeds and discovery recorded
  a file failure, loading returns that failure.
- The standalone administrative parser is named `cli-defs`, so its actual
  namespace is `[cmds.cli-defs]` and `MXD_CMDS_CLI_DEFS_*`. Pin that name to
  preserve the existing contract; credentials remain positional.

## Decision log

- 2026-10-02: Centralize the caret `0.9.0` requirement with defaults disabled
  and JSON and TOML enabled explicitly. Root and cli-defs format features must
  forward to OrthoConfig; root forwards formats to both normal and build
  cli-defs.
- 2026-10-02: Preserve direct parsers only for independent application use.
  The validator's TOML reader is independent; retain it. Preserve genuine
  Figment test/provider dependencies.
- 2026-10-02: Implement dependency and runtime migration as one coherent
  plateau. Do not introduce temporary compatibility wrappers for private APIs.

- 2026-10-02: Set cli-defs' compiler floor to upstream's 1.89.0 and MXD's
  floor to 1.92.0, required by the existing locked PostgreSQL tooling.
- 2026-10-02: Merge generated lower layers with the already parsed flattened
  CLI fields. Rename the derive-generated flatten group to private
  `__AppConfigCli`; `Cli::resolve_config` reconstructs only the `--config-path`
  selector, sanitizes the parsed group, and appends it as the final merge
  layer. This eliminates raw argv rescanning while preserving explicit values
  equal to defaults.

## Outcomes & retrospective

Implementation and isolated format/compiler/build-time validation are complete.
All canonical deterministic gates pass. Migration commit b5d6886 is complete.
CodeRabbit completed with one minor documentation concern; its correction and
follow-up review precede publication. Hosted CI has not run for this branch.

## Context and orientation

Current HEAD uses the published OrthoConfig 0.9.0 requirement from the
workspace dependency table. `cli-defs/src/lib.rs` defines `AppConfig`, the
private generated `__AppConfigCli` flatten group, `Cli`, `Commands`, and
`CreateUserArgs`. `Cli::resolve_config` passes only the reconstructed
`--config-path` selector to the generated lower-layer loader, then sanitizes
the already parsed flatten group into the final CLI layer. The subcommand and
its credential tokens are never rescanned from raw process arguments.
`build.rs` still consumes `Cli::command` for man-page generation only. The
admin path continues to merge the selected `create-user` namespace and restore
an explicitly supplied password after serialization.

The current feature graph below is confirmed by normal/development/build
`cargo tree` checks for all three base lanes and their JSON5/YAML variants:

- The workspace `ortho_config` dependency disables upstream defaults and
  enables `serde_json` and `toml`. The published 0.9.0 file parser refers to
  the TOML provider unconditionally, so TOML stays available even when
  `cli-defs` is built with `--no-default-features`.
- `cli-defs` defaults to TOML and forwards its explicit `toml`, `json5`, and
  `yaml` features to OrthoConfig.
- Root `toml`, `json5`, and `yaml` features forward to OrthoConfig and to
  the `cli-defs` package. Both runtime and build-time `cli-defs` edges disable
  that package's defaults; the workspace dependency preserves the TOML
  provider. JSON5 and YAML remain opt-in.
- The lockfile resolves the MXD-owned integration to OrthoConfig 0.9.0. It also
  retains registry OrthoConfig 0.5.0 through `pg-embed-setup-unpriv`; that
  independent dependency is not an MXD-owned requirement and is not forced
  across versions.

Before this change, root and `cli-defs` selected Git tag v0.3.0 and carried
direct Figment format providers, `serde_yaml`, `uncased`, and XDG support for
generated configuration. The old CLI loader parsed process arguments again and
stripped apparent command tokens. The standalone CLI package defaulted to TOML,
but root no-default builds did not disable the dependency's defaults.

The target and current policy preserve TOML as the baseline format in all
lanes; JSON5 and YAML are opt-in. YAML uses the Saphyr-backed YAML 1.2
provider, so `yes`, `no`, `on`, and `off` remain strings, `true` and `false`
are booleans, and duplicate mapping keys fail.

## Conformance basis

- Issue #574: dependency/compiler (R1), format/build graph (R2), server/admin
  semantics (R3), discovery/environment boundary (R4), docs/lanes (R5).
- Issue #509: child-process or explicit map/provider environment taxonomy.
- Issue #507: canonical backend Make/CI gates already in this checkout.
- Tagged upstream v0.9.0 Cargo manifests, documentation index, and v0.6.0,
  v0.7.0, v0.9.0 migration guides.
- Current repository AGENTS.md, users-guide.md, developers-guide.md, and
  documentation-style-guide.md. No separate migration ToR or design exists.

## Verification plan

R1 -> M1/M2 -> locked metadata, cargo tree normal/dev/build, compiler checks.
R2 -> M1/M2 -> standalone cli-defs default/no-default/individual formats,
host/target graphs, equivalent fixtures, YAML scalar/duplicate-key tests. R3 ->
M1 -> subprocess precedence tests for all six fields, explicit default values,
positional command-name values, separators, omitted overrides; credential
exclusion/override tests and disposable-database BDD scenarios. R4 -> M1 ->
injected MapEnv discovery tests and isolated generated-loader subprocess tests
for absent/malformed/unreadable/inherited files. R5 -> M2 -> generated help/man
assertions plus canonical Make gates.

Each negative fixture must fail on the real integration boundary; comparisons
must inspect resolved fields or typed errors rather than parse human messages.
External assumptions: shipped OrthoConfig/clap/provider APIs, local filesystem,
and disposable embedded PostgreSQL/SQLite test infrastructure. This migration
introduces no new business-logic lemma, concurrency protocol, or unsafe code;
finite contract partitions and a parsing property test provide proportionate
verification without attempting to prove third-party implementations.

## Plan of work

The locked API review, baseline graph inspection and red argv regression are
complete. The implementation upgrades both manifests, removes redundant
generated-support imports, and composes generated lower layers with the
sanitized, already parsed `__AppConfigCli` fields through
`Cli::resolve_config`. It reconstructs only the config selector and keeps
positional credentials and the `--` boundary in the top-level parse.

Move migration-touched environment tests to a child-process harness with an
empty environment and disposable working directory. Protect credentials and
validate configuration before infrastructure operations. Update docs with
actual discovered-file policy, supported formats, and compiler evidence.

## Milestones and plateaus

M1 satisfies R1-R4 with compiling normal and build consumers, preserved CLI
contracts, and deterministic regression coverage. Gate all applicable lanes
before committing and requesting CodeRabbit. Recovery: revert the coherent
migration commit. Remaining: expanded matrix/documentation evidence in M2.

M2 satisfies R1-R5 with independent format/compiler/host-target checks, current
documentation, and recorded evidence. Repeat gates affected by changes, commit,
and clear review before publication. Recovery: revert M2 independently.
Compatibility commitment: existing user configuration files and positional CLI
credentials; no temporary source compatibility shim is required.

## Concrete steps

Run all commands from the repository root and capture gates with pipefail and
tee to /tmp. Delegate sequential gates and foreground CodeRabbit to scrutineer.

- `make check-fmt`, `make typecheck`, `make lint`, `make test`.
- `make markdownlint`, `make nixie`, and applicable workflow contracts.
- `make test-cli-defs`: default, no-default, each declared format, and combined
  format cases. TOML remains compiled in the no-default case because the
  published provider is an unconditional dependency of this integration.
- `make check-config-msrv`: check `cli-defs` at Rust 1.89.0 with all format
  features and check the PostgreSQL, SQLite, and wireframe-only workspace
  consumers at Rust 1.92.0.
- Inspect `cargo tree -e normal,build,dev,features`; the expected MXD-owned
  provider graph has TOML and JSON support at baseline with JSON5/YAML
  individually opt-in.
- Canonical backend formats exercised without enabling both databases.
- `coderabbit review --agent` after deterministic gates succeed.

## Validation and acceptance

Record each actual command, exit status, log path, and current commit. Keep
baseline failures distinct from migration failures and hosted CI. A queued
review is not a completed review. All #574 acceptance items must have evidence
before reporting completion; secrets must remain excluded from serialized
configuration and diagnostic dumps.

## Idempotence and recovery

Feature checks and tests may be repeated without changing real databases. Use
disposable paths and test credentials. Preserve the targeted lockfile diff for
review, avoid global Cargo-cache workarounds, and never kill other jobs.

## Artefacts and notes

Baseline HEAD and experiment evidence are recorded below. Add post-change gate
results here as they become available. Plan revisions must update affected
progress, risks, decisions, and verification obligations.

## Interfaces and dependencies

Use the published OrthoConfig derive re-export, existing clap flatten types,
and runtime configuration adapters. Maintain selected create-user merging and
password restoration. Keep build.rs metadata-only and validator seams intact.

Revision note (2026-10-02): initial authorized implementation plan captures the
v0.3.0 baseline, complete integration scope, and evidence still required.

## Evidence collected during implementation

Baseline `make check-fmt`, `make typecheck`, and `make lint` passed after
formatting the initial plan. Logs: `/tmp/mxd-574-baseline-check-fmt-2.out`,
`/tmp/mxd-574-baseline-typecheck.out`, and `/tmp/mxd-574-baseline-lint.out`.
Baseline `make test` stopped in PostgreSQL with
`postgres_category_names_are_bundle_scoped`: embedded PostgreSQL could not
connect to its admin database (30 passed, one failed, 457 not run). See
`/tmp/mxd-574-baseline-test.out`; the cause remains unresolved and the failure
is not migration evidence.

The original loader failed the black-box database-value regression, recorded in
`/tmp/mxd-574-red-argv-rerun.out`. The published provider limitation is
recorded in `/tmp/mxd-574-no-toml-experiment.out`. The targeted lock update
removed MXD-owned Git v0.3.0 entries and retained independently owned registry
v0.5.0 through pg-embed-setup-unpriv; see `/tmp/mxd-574-targeted-update.out`.
The new `make test-cli-defs` and `make check-config-msrv` targets are
conditionally called from the SQLite and PostgreSQL CI matrix lanes,
respectively. Migration gates, these new targets, and post-change PostgreSQL
tests remain pending.

Revision note (2026-10-02): implementation notes now record the measured
OrthoConfig 0.9.0 feature floor, private generated CLI group composition,
compiler-validation targets, and conditional CI placement. Gate evidence
remains pending; the PostgreSQL baseline failure is still unresolved.

Focused migration evidence now passes: standalone default cli-defs (29 tests),
standalone no-default with named TOML/JSON5/YAML formats (33 tests), and
executable configuration/admin contracts (13 tests). Logs:
`/tmp/mxd-574-cli-defs-default-4.out`, `/tmp/mxd-574-cli-defs-formats-2.out`,
and `/tmp/mxd-574-configuration-cli.out`. YAML duplicate rejection reports a
typed provider gathering/merge error rather than the file variant; the test
matches semantic variants without parsing messages.

`make check-config-msrv` passed all four compiler-floor checks, including
cli-defs all targets at Rust 1.89.0 and MXD all targets in each canonical lane
at Rust 1.92.0. See `/tmp/mxd-574-check-config-msrv.out`.

The failing baseline PostgreSQL schema test passed with a fresh disposable
`PG_DATA_DIR` under `.pg-embedded/issue-574-probe`, using the same fixed test
password and shared binary cache. See `/tmp/mxd-574-postgres-probe.out`. This
implicates prior fixture state without proving its exact cause. Full migration
tests will use `.pg-embedded/issue-574-gates`; no existing data directory or
shared cache is deleted. Full canonical gates remain pending.

The initial full PostgreSQL run reached 375 passing tests and stopped on the
new help assertion: v0.9.0 hides the generated `--config-path` selector by
default. MXD now opts into `discovery(config_cli_visible = true)` so its help
and generated man pages expose the documented selector. See
`/tmp/mxd-574-m1-test-retry.out`; the full suite must be rerun after this fix.
Clippy's fallible-test assertions were converted to `ensure!` and contextual
errors, preserving strict lint policy without suppressions.

The full PostgreSQL lane passed 488/488 tests. The subsequent SQLite failure
exposed an incorrect new test assumption: the shipped administrative helper
uses conventional file candidates and does not consume the server's
`MXD_CONFIG_PATH` selector. The v0.3.0 helper had this same separation. Keep
that contract, document it, and test equivalent enabled formats with their
conventional `.mxd.*` filenames while explicitly selecting the same file for
server settings. See `/tmp/mxd-574-m2-test.out`. No runtime selector forwarding
or credential serialization change is introduced.

Strict public rustdoc exposed four pre-existing links to the private
`MigrationTimeoutError` type (both backends), confirmed unchanged in original
HEAD b52f971. Convert those links to inline code without changing visibility,
error types, or migration behaviour. This documentation-only prerequisite is
separate from dependency repair. See `/tmp/mxd-574-m4-rustdoc-postgres.out`;
rerun each canonical rustdoc lane.

## Final deterministic validation receipt

The migration passes PostgreSQL (488 tests), SQLite (498 tests), and
wireframe-only (496 tests) suites. PostgreSQL uses the fresh disposable
`.pg-embedded/issue-574-gates` fixture; the original fixture failure's exact
cause remains unproved. Logs: `/tmp/mxd-574-m2-test.out`,
`/tmp/mxd-574-m3-test-sqlite.out`, and
`/tmp/mxd-574-m3-test-wireframe-only.out`. Current executable format/admin
fixtures pass all 16 tests after moving equivalent data into external files.

The six standalone cli-defs default/no-default/individual/combined format
combinations pass (`/tmp/mxd-574-m2-test-cli-defs.out`). Compiler-floor
validation passes exactly four commands: cli-defs all targets at 1.89.0, and
MXD all targets at 1.92.0 in PostgreSQL, SQLite, and wireframe-only lanes
(`/tmp/mxd-574-m2-check-config-msrv.out`). These MXD compiler checks select the
root package rather than claiming every workspace member's MSRV.

Locked metadata, canonical typechecking, Clippy, and Whitaker pass. Additional
Clippy checks with JSON5/YAML pass in each backend lane, as does standalone
cli-defs format-enabled Clippy. Strict workspace rustdoc passes all three lanes
(`/tmp/mxd-574-m6-rustdoc-*.out`); configuration format Clippy receipts are
`/tmp/mxd-574-m6-clippy-formats-*.out`. Formatting, Markdown lint, Mermaid
validation, Makefile validation, workflow contracts (172 tests), verification
(69 passed, one skipped), concurrency tests, and doctests pass.

Pre-existing public rustdoc links were repaired in documentation-only commit
2bea181. The pinned Markdown gate regenerated spelling configuration in
separate commit 88f0649; that generated file was not hand-edited. These
prerequisite commits neither change runtime behaviour nor refresh unrelated
dependencies.

Actual before/after graph evidence is in `/tmp/mxd-574-before-features.out` and
`/tmp/mxd-574-graph-{pg,sqlite,wireframe}-{base,formats}.out`. Every MXD-owned
normal/build integration resolves to 0.9.0 with JSON and TOML; enabled
JSON5/YAML reach both host and target consumers. No v0.3.0 runtime or macro
remains. Independently owned v0.5.0 is isolated through pg-embed-setup-unpriv
(`/tmp/mxd-574-graph-independent-ortho-config-0_5.out`).

Cargo build-script messages identify the actual generated `mxd.1` outputs for
default and wireframe plus formats configurations. Both expose version,
configuration selector, six settings, and create-user. Receipts:
`/tmp/mxd-574-man-default-build-messages.jsonl`,
`/tmp/mxd-574-man-wireframe-formats-build-messages.jsonl`, and
`/tmp/mxd-574-man-rendered-content.out`. build.rs remains metadata-only.

Revision note (2026-10-02): isolated/compiler/documentation evidence was
completed before the first major review, strengthening M1's validation rather
than postponing compatibility checks until M2. Review and publication are the
remaining work.

## Review dispositions

CodeRabbit completed the M1 review of all 29 changed files, matching the
`origin/main` diff despite reporting `main` in its metadata. Its sole finding
requested third-person phrasing for the explicit file selector in the design
guide. The correction preserves the required-file semantics; no runtime or test
changes are needed. Receipt: `/tmp/mxd-574-review-m1.out`. Documentation gates
and follow-up review must pass before M1 is marked complete.

The explicit-base follow-up review completed against b52f971 and reported two
minor prose issues: the matching selector sentence in the users' guide and a
neither/nor construction in this plan. Both are corrected without changing
configuration behaviour. Receipt: `/tmp/mxd-574-review-m1-followup.out`. The
follow-up documentation gates and review remain the publication condition.
