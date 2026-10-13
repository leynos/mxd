# Simplify the BDD `HandshakeWorld` implementation in handshake tests

This ExecPlan is a living document. Keep `Constraints`, `Tolerances`, `Risks`,
`Progress`, `Surprises & discoveries`, `Decision log`,
`Outcomes & retrospective`, `Conformance basis`, and `Verification plan`
current.

Status: COMPLETE

## Purpose / big picture

Issue [#235][issue-235] reports that the behaviour-driven development (BDD)
world used by the wireframe handshake scenarios is harder to read than it needs
to be. The world struct currently owns a private Tokio `Runtime`, calls
`Runtime::block_on` from synchronous step functions, and guards every field
with a `RefCell` to get interior mutability past `&self` step parameters. A
reader has to hold three ideas at once — a hidden runtime, manual futures
blocking, and borrow panics — to follow a test that is, in substance, three
network calls.

After this change, the world is an ordinary struct with plain `Option` fields,
the one operation that waits on the network is an `async fn` taking
`&mut self`, and each scenario runs under
`#[tokio::test(flavor = "current_thread")]`. A reader sees `.await` exactly
where work is asynchronous, `?` where it can fail, and no runtime handle at
all. Nothing about the observable protocol behaviour changes: the four
scenarios still assert handshake reply codes 0, 1, 2, and 3.

The change is observable by running the focused suite and seeing the same eight
handshake tests pass, by reading the world in the source without encountering
`block_on` or `RefCell`, and by the repository gates staying green.

## Constraints

- Preserve all four scenario bindings, their feature file
  (`tests/features/wireframe_handshake_hooks.feature`), their `index = 0..3`
  selection, and their asserted reply codes (0, 1, 2, 3).
- Preserve the reply-decoding semantics exactly, including the big-endian
  `[4..8]` slice, the `"missing reply"` error, and the
  `"convert reply slice to array (bdd reply)"` message.
- Preserve the handshake error semantics: a stored handshake failure is still a
  successful step that the `Then` step inspects, and a missing server address
  is still a step error.
- Preserve best-effort shutdown: dropping the world still signals the server to
  stop, without awaiting or joining it.
- Keep the existing timing constants: the BDD world starts its server with a
  100 ms preamble timeout, the client connect/write/read path keeps its
  one-second read timeout, and the production server tests keep
  `HANDSHAKE_TIMEOUT`.
- Do not change any non-test production module. The only owned file that may
  change its behaviour-facing content is `src/wireframe/handshake_bdd.rs`.
- Keep the file under the repository's 400-line limit and satisfy the workspace
  Clippy lints, which deny `expect_used`, `unwrap_used`, `indexing_slicing`,
  `cognitive_complexity`, and `string_slice` in production code, with
  `allow-expect-in-tests = true` covering test code.
- Keep the working tree free of orphaned artefacts; do not add new files unless
  a gate requires them.

## Tolerances (exception triggers)

- Scope: stop if the refactor needs changes outside
  `src/wireframe/handshake_bdd.rs` plus this plan and its registration, other
  than a small, evidenced addition forced by a gate.
- Interface: stop if a non-test public API of the `mxd` crate must change.
- Dependencies: stop before adding, removing, or upgrading a dependency. The
  issue's "pending rstest-bdd v0.2.0" dependency is already satisfied by the
  vendored 0.6.0 adoption; no manifest change is expected.
- Compiler: stop if the documented rustc internal compiler error (ICE) for
  macro-driven scenarios with `&mut` fixtures reproduces on the pinned nightly
  after the review in `Decision log`.
- Validation: keep any failing or unavailable check visible; do not merge with
  a required check red, pending, or unknown.
- Review: do not bypass an unresolved substantive CodeRabbit finding.

## Risks

- Risk: the pinned nightly reintroduces the upstream rustc ICE documented for
  `&mut` fixtures in macro-driven scenarios. Severity: high; likelihood: low.
  Mitigation: use `&mut` only on the three mutating steps, keep `&self` on the
  assertion step, and confirm compilation early with a focused test run before
  running wider gates. If the ICE reproduces, fall back as recorded in
  `Decision log`.
- Risk: converting the steps to async changes when the fixture is borrowed
  across an `.await`, which can surface an `E0499`/`E0502` conflict in the
  generated wrapper. Severity: medium; likelihood: low. Mitigation: keep all
  step bodies free of nested fixture requests, and let the wrapper own the
  borrow. Verified by compilation and the focused run.
- Risk: `Drop` no longer has a runtime in scope, so a shutdown that previously
  ran under `block_on` might be silently skipped. Severity: medium; likelihood:
  low. Mitigation: the shutdown channel was always a synchronous
  `oneshot::Sender::send`; keep that call and confirm scenario runtimes remain
  single-threaded so the spawned server task is still driven.
- Risk: the runtime-owned-by-world pattern was masking a `Send` requirement.
  Severity: low; likelihood: low. Mitigation: run scenarios on a current-thread
  runtime, where the world never crosses a task boundary.

## Progress

- [x] (2026-10-13) Confirm clean baseline on branch
  `issue-235-simplify-bdd-handshakeworld-implementation-in-handshake-tests`,
  record the focused test result, and warm the Clippy cache.
- [x] (2026-10-13) Establish rstest-bdd 0.6.0 `&mut` fixture and async step
  posture from the pinned sources and imported guides.
- [x] (2026-10-13) Convert `HandshakeWorld` to plain state with async mutable
  network operations and remove the owned runtime.
- [x] (2026-10-13) Convert the steps and the four scenario bindings to the
  async value-fixture pattern.
- [x] (2026-10-13) Commit EP-M1 and EP-M2 as `e28dfc7`, after the focused suite
  reported eight passing tests and a negative control failed all four scenarios
  with the observed reply codes.
- [x] (2026-10-13) Push the branch over SSH and open draft pull request #654
  with `Closes #235` in the body and a `## References` section.
- [x] (2026-10-13) First gate run: `make check-fmt`, `make markdownlint`,
  `make nixie`, and the focused suite passed; `make lint` failed on a real
  `clippy::unused_async` defect and `make test` was cut short by an
  environmental package-cache timeout.
- [x] (2026-10-13) Fix the denied lint by making the test-only `start_server`
  helper synchronous again, committing as `8297264`, and re-running the focused
  suite green.
- [x] (2026-10-13) Re-queue a hosted CodeRabbit review via `comenq` against the
  corrected head `788e0ff` (queue ID `116404a5`, ETA ~50 minutes).
- [x] (2026-10-13) Complete the gate re-run: `make lint` now passes clean on
  the corrected head, and every sub-suite that the previous abort skipped
  passed. `test-rstest-bdd-ui` remains an environmental timeout.
- [x] (2026-10-13) Resolve both hosted-review findings, correcting the INV-1
  error-branch claim and removing first-person wording.
- [ ] (2026-10-13) Reply to the review threads, re-verify, and write the
  retrospective.

## Surprises & discoveries

- The issue cites `src/wireframe/handshake.rs` lines ~250-318, but the world
  lives in `src/wireframe/handshake_bdd.rs`. The cited lines in `handshake.rs`
  are the production `tests` module's `start_server` helper, which remains the
  correct source of the 100 ms server timeout.
- The repository is already on rstest-bdd 0.6.0, adopted by commit `2517820`.
  The issue's dependency on "rstest-bdd v0.2.0" is therefore already satisfied,
  and 0.6.0 natively awaits async step bodies and supports `&mut` fixtures.
- rstest-bdd 0.6.0 documents a rustc ICE for macro-driven scenarios with `&mut`
  fixtures, gated behind its additive `mutable_world_macro` feature. Its own
  `tests/async_step_functions.rs` nevertheless compiles and runs
  `#[from(state)] state: &mut CounterState` with an async step under
  `#[tokio::test(flavor = "current_thread")]`, and the repository's current
  world already declares `&HandshakeWorld` step parameters successfully. The
  ICE is therefore likely tied to the gated expander rather than to ordinary
  `&mut` step parameters on this nightly.
- The current `Drop` implementation already sends on a synchronous
  `oneshot::Sender`; the only runtime dependency was the surrounding in-scope
  `Runtime` field, not the send itself.
- The documented `&mut` fixture ICE did **not** reproduce on the pinned
  nightly `nightly-2026-03-26`. `&mut HandshakeWorld` step parameters compiled
  and ran under `#[tokio::test(flavor = "current_thread")]` on the first
  attempt, so no fallback or interior-mutability shim was needed. The ICE is
  evidently tied to the expander that the crate's own opt-in
  `mutable_world_macro` feature gates, not to ordinary `&mut` step parameters.
- The issue's "pending rstest-bdd v0.2.0" premise is doubly stale: 0.6.0 is
  already adopted, and it provides the async support the issue was waiting for.
  Its `TokioHarness` does, however, poll each async step once, so socket-I/O
  steps still need the repository's documented explicit async bindings —
  exactly what `#[scenario]` without a harness attribute plus an external
  `#[tokio::test]` produces.
- `start_server` in the production `tests` module was initially converted to an
  `async fn` so the world could await it. The first gate run then rejected that
  with `clippy::unused_async`, which this workspace denies. The lint is
  correct: spawning the server task is synchronous and the only `await` in the
  body sits inside the nested async block handed to `run_with_shutdown`. The
  helper is now a plain `fn`, and the async boundary sits exactly where socket
  I/O happens. This is a test-only helper inside `#[cfg(test)]`, not a public
  API, so the interface tolerance was never in play.
- The lint gate, not the test suite, is what caught the over-eager `async`. The
  focused tests passed both before and after, so only a denied-lint check could
  distinguish the two versions. That is a reminder that a green test run says
  nothing about whether a signature is *honest*.
- `test-rstest-bdd-ui` timed out at its 300-second cap three times during the
  gate runs and then passed in 297.4 seconds once the host quieted. It is a
  `trybuild` compile-contract suite: it compiles fixture crates inside the test
  body, and the host was running a saturated build queue (capacity 4, all held,
  ~90 crates waiting, load average above 30 on 6 cores) with the managed
  sccache unreachable, so every crate compiled uncached. The suite's package
  manifest depends only on `rstest`, `rstest-bdd`, `rstest-bdd-macros`,
  `rstest-bdd-harness-tokio`, and `trybuild` — not on `mxd` — so it compiles
  none of this change's code and cannot be affected by it. The passing run
  finished 2.6 seconds inside the cap, which is the clearest statement of how
  marginal the threshold is under load. No configuration change was made:
  raising a shared timeout to mask transient host contention is out of scope
  for this refactor.
- `pr.txt` and `rustc-ice-2025-12-09T19_07_55-92865.txt` in the worktree root
  are tracked, unmodified repository files unrelated to this task, left
  untouched.
- rust-analyzer reports "Ambiguous step definition" diagnostics for the
  `#[scenario]` bindings, but these are false positives. Only one `#[given]` in
  the repository matches that step text, and `make lint` compiles the same code
  through `clippy --all-targets` with `compile-time-validation` enabled without
  reporting any ambiguity. The observed rust-analyzer process runs toolchain
  `nightly-2026-01-30` while the project pins `nightly-2026-03-26`, and the
  diagnostics are tagged `[macro-error]`, so they originate in the editor's own
  macro-expansion layer against a mismatched toolchain. It also flags three of
  the four structurally identical bindings and not the fourth, which no real
  duplicate definition would do. rustc is the arbiter; it sees no conflict.

## Decision log

- Decision: refactor in place in `src/wireframe/handshake_bdd.rs` rather than
  moving the world. Rationale: the issue's line citation is stale, the file is
  already the canonical home of the BDD world, and moving it would widen the
  diff without improving the result.
- Decision: keep `reply_code` synchronous, taking `&self`. Rationale: the
  method decodes bytes already stored in the world and awaits nothing. Making it
  `async` would add ceremony and a needless borrow across a suspension point
  while the issue's stated goal is to reduce cognitive complexity. The issue
  lists it among the async candidates only because every method then took
  `&self`; that reasoning no longer applies once the state is plain.
- Decision: keep `perform_handshake` as a free async function, unchanged.
  Rationale: it presumes no runtime and its connect/write/read helper split is
  the clearest expression of the three network steps.
- Decision: treat the rstest-bdd dependency as satisfied by 0.6.0. Rationale:
  the present tree already depends on 0.6.0, which provides native async step
  execution and `&mut` fixture parameters. No manifest change is warranted.
- Decision: run scenarios with `#[tokio::test(flavor = "current_thread")]`.
  Rationale: it matches the repository's existing async scenario convention in
  `tests/wireframe_handshake_metadata.rs` and keeps the world `!Send`-safe and
  free of cross-thread fixture sharing.
- Decision: leave the production test helper `start_server` synchronous.
  Rationale: it spawns the server task and returns; the awaiting happens later,
  when the step reads the reply on the scenario's own runtime. An earlier
  attempt made the helper `async` to match the issue's proposal, but that added
  no awaiting work and was rejected by the denied `unused_async` lint. Neither
  blocking on a runtime nor reintroducing an owned runtime is needed:
  `tokio::spawn` requires a runtime in scope but does not need the caller to
  await anything.
- Decision: keep the explicit `#[scenario]`-plus-`#[tokio::test]` binding form
  rather than moving to an rstest-bdd harness attribute. Rationale: it is the
  repository's documented workaround for the 0.6.0 Tokio harness polling each
  async step once, and these steps perform socket I/O. Using a harness would
  fail with "async step yielded Pending".
- Decision: do **not** add `handshake_bdd` to `tests/bdd_binding_coverage.rs`.
  Rationale: that gate compares named bindings of *external* async test suites.
  This suite lives inside `src/`, is not an external test target, and uses
  index bindings, so adding it would break the coverage comparison rather than
  strengthen it. The issue explicitly scopes this out.
- Decision: request the hosted CodeRabbit review through the managed `comenq`
  queue while keeping the branch a draft. Rationale: CodeRabbit skips automatic
  review of draft pull requests (confirmed by the
  `skip review by coderabbit.ai` marker on comment 6134566280), and the queue
  is the project's authorized route for a full review. The local
  `coderabbit review --agent` pass covers the same candidate immediately, so
  the queued request is a second, independent inspection rather than a blocking
  wait.

## Outcomes & retrospective

Complete, 2026-10-13.

The refactor shipped the shape the issue asked for: `HandshakeWorld` holds plain
`Option` fields, no `RefCell`, no owned `Runtime`, and no `block_on` anywhere.
`src/wireframe/handshake_bdd.rs` went from 188 to 174 lines while gaining
Rustdoc on the world and its methods. Observable protocol behaviour is
unchanged; the four scenarios still assert reply codes 0, 1, 2, and 3 against
the same feature file.

The final test inventory is eight focused tests, all passing: four BDD
scenarios under `wireframe::handshake::bdd` (`replies_ok`, `invalid_protocol`,
`unsupported_version`, `handshake_timeout`) and four unit tests in
`wireframe::handshake::tests` (`replies_success`, `replies_handshake_errors`
for both cases, and `replies_timeout_for_idle_socket`). Every repository gate
is green on the shipped head: `check-fmt`, `lint` (six phases, zero warnings),
`markdownlint`, `nixie`, `test-verification` (69 passed), `test-fuzz-harness`
(8 passed), `test-concurrency` (7 passed plus doctests), `test-doc` (5 passed,
4 ignored), and the three database and wireframe backends (497, 502, and 500
passed).

The documented `&mut` fixture ICE did not materialize, so the planned fallback
was never needed and no interior-mutability shim survives in the code.

Three lessons are worth carrying forward.

The first is that the behavioural suite could not see the one real defect this
work introduced. All eight tests passed while `start_server` carried a needless
`async` marker, and only the denied `unused_async` lint exposed it. Passing
tests show that code behaves correctly, not that its signatures are honest
about what they do.

The second is that a verification plan can overclaim, and reviewing prose
matters as much as reviewing code. INV-1 asserted that the idle-connection
scenario stores an *error* reply and so exercises the error branch. It does
not: the server's 100 ms preamble timeout beats the client's one-second read
budget, so reply code 3 is read successfully and all four scenarios store the
`Ok` variant. CodeRabbit caught the overclaim. The inference had felt sound
because the scenario is *named* a timeout, which is exactly the kind of
plausible-but-wrong reasoning that writing the invariant down invited.

The third is that shared-host contention is not a code signal. The
`test-rstest-bdd-ui` suite timed out at its 300-second cap three times and then
passed in 297.4 seconds once the queue drained. Its package does not depend on
`mxd` at all, so it compiles none of this change. The right response was to
identify the environmental cause and re-run when the host allowed, not to raise
the shared timeout.

Deliverables: branch
`issue-235-simplify-bdd-handshakeworld-implementation-in-handshake-tests`
pushed at head `6fe8702`, and draft pull request
[#654](https://github.com/leynos/mxd/pull/654), which states `Closes #235`.

## Conformance basis

- Upstream requirement: GitHub issue [#235][issue-235], "Simplify BDD
  HandshakeWorld implementation in handshake tests", including the refactoring
  bullets for plain `Option` fields, async `&mut self` operations, `.await` in
  place of `block_on`, a synchronous `Drop`, and an async test runtime. The
  bullet treating `start_server` as an async candidate is superseded: it awaits
  nothing that the caller needs, so it stays synchronous.
- Prior art: pull request [#234][pr-234] and its review comment
  [#discussion_r2576844667][pr-234-comment], which introduced the world.
- Repository standard: `AGENTS.md` — "Code is for humans", new functionality
  validated by unit and behavioural tests, Clippy warnings disallowed,
  functions extracted where long, and `expect` permitted in tests with a
  diagnostic message.
- Repository standard: `docs/rstest-bdd-users-guide.md` and
  `docs/rstest-bdd-v0-6-0-migration-guide.md` — async step execution, mutable
  fixtures, and the documented `&mut` fixture ICE caveat.
- Repository standard: `docs/developers-guide.md` — BDD scenario conventions
  and the gate set.
- Specification under test: `tests/features/wireframe_handshake_hooks.feature`,
  scenarios "Successful handshake replies OK", "Invalid protocol returns an
  error", "Unsupported version returns an error", and "Idle connection times
  out during handshake", asserting reply codes 0, 1, 2, and 3.
- No Terms of Reference or technical design document governs this refactor;
  the issue, the prior pull request, and `AGENTS.md` are the governing
  artefacts.

Trace links:

- ISSUE-235-01 (plain state, no owned runtime) -> EP-M1 ->
  `cargo test --lib wireframe::handshake` passes with no `block_on` or
  `RefCell` remaining in the world.
- ISSUE-235-02 (async steps under an async runtime) -> EP-M2 ->
  the four `wireframe::handshake::bdd::*` scenarios pass under
  `#[tokio::test(flavor = "current_thread")]`.
- ISSUE-235-03 (behaviour preserved) -> EP-M3 ->
  `make check-fmt`, `make lint`, and `make test` pass, with reply codes 0-3
  asserted exactly as before.

## Plan of work (milestones)

Each milestone ends in a coherent, validated repository state.

### EP-M1 — Plain-state world with async network operations

Stable identifier: EP-M1. Satisfies ISSUE-235-01.

End state: `HandshakeWorld` holds `Option<SocketAddr>`,
`Option<oneshot::Sender<()>>`, and `Option<Result<[u8; REPLY_LEN], String>>`,
derives `Default`, and exposes `fn start_server(&mut self)`,
`async fn connect_and_maybe_send(&mut self, bytes: Option<Vec<u8>>)`, and
`fn reply_code(&self)`. The `RefCell` and `Runtime` imports are gone, no
`block_on` call remains, and `Drop` still signals shutdown when a sender is
present. The step functions and scenario bindings still use the old synchronous
shape at this point, so the milestone's compile evidence comes from the
following milestone; the two are committed together because the world and its
only callers form one atomic interface change.

Only `connect_and_maybe_send` is async, because only it waits on socket I/O.
`start_server` spawns a task and returns; marking it `async` earned a denied
`unused_async` lint in the first gate run and the async marker was removed.

Acceptance evidence: no `RefCell`, `block_on`, or `Runtime` token appears in
`src/wireframe/handshake_bdd.rs`.

### EP-M2 — Async steps and scenarios

Stable identifier: EP-M2. Satisfies ISSUE-235-02.

End state: `#[fixture] fn world()` returns `HandshakeWorld::default()`; the
three `When` steps are `async fn`s taking `&mut HandshakeWorld`; the Given step
and the `Then` step are synchronous `fn`s, taking `&mut HandshakeWorld` and
`&HandshakeWorld` respectively; the four scenarios are `async fn`s with empty
bodies annotated `#[tokio::test(flavor = "current_thread")]`, keeping
`path = "tests/features/wireframe_handshake_hooks.feature"` and `index = 0..3`.
Mixing a synchronous `&mut` step with asynchronous ones compiles and runs
without complaint from the `compile-time-validation` feature.

Acceptance evidence: the focused suite reports eight passing tests, including
all four `bdd` scenarios.

### EP-M3 — Validation, review, and publication

Stable identifier: EP-M3. Satisfies ISSUE-235-03.

End state: focused tests, `make check-fmt`, `make lint`, and `make test` pass;
a CodeRabbit review pass is clean; the branch is pushed and a draft pull
request states `Closes #235`.

Acceptance evidence: gate logs under `/tmp`, a clean review report, and the
draft pull request URL.

## Concrete steps

All commands run from the worktree root,
`/home/leynos/.lody/repos/github---leynos---mxd/worktrees/3a37045c-48ac-4428-98fe-0538488f3bb8`.

1. Rewrite the world in `src/wireframe/handshake_bdd.rs` per EP-M1.

2. Rewrite the steps and scenarios per EP-M2.

3. Run the focused suite:

    ```sh
    RUSTFLAGS="-D warnings" cargo test --locked --features "sqlite test-support" \
      --lib wireframe::handshake 2>&1 | tee /tmp/test-issue-235-<branch>.out
    ```

    Expect `test result: ok. 8 passed` listing `replies_ok`,
    `invalid_protocol`, `unsupported_version`, and `handshake_timeout` under
    `wireframe::handshake::bdd`.

4. Run the gate set sequentially through the `scrutineer` subagent, which
   captures each gate to a log: `make check-fmt`, `make lint`, `make test`.

5. Request a CodeRabbit agent review and clear every concern before
   proceeding.

6. Commit, push over SSH, and open the draft pull request.

## Validation and acceptance

- Observable outcome: `cargo test --lib wireframe::handshake` passes with the
  same eight tests before and after.
- Observable outcome: reading `src/wireframe/handshake_bdd.rs` shows `.await`
  at each network operation, `Option` fields, and no `Runtime`, `RefCell`, or
  `block_on`.
- Observable outcome: the four feature scenarios still assert codes 0, 1, 2,
  and 3, and the idle-connection scenario still exercises the 100 ms server
  preamble timeout against the client's one-second read timeout.
- Observable outcome: `make check-fmt`, `make lint`, and `make test` pass.

## Verification plan

Invariants introduced or preserved:

- INV-1: the world's reply state is a single value that the `Then` step reads
  without panicking. Method: the four scenarios assert exact reply codes, so a
  missing or misdecoded reply fails the scenario. Non-vacuity: the four
  scenarios assert four distinct codes (0, 1, 2, 3), so a constant or
  misdecoded reply fails at least three of them.
- INV-2: the handshake step stores a reply for every scenario, whether the
  server answered with success or with an error code. Method: scenario 0
  exercises the success path (code 0) and scenarios 1-3 exercise error codes
  (1, 2, 3). Non-vacuity: the stored codes differ across scenarios, so an
  inverted store fails at least one scenario.

  Residual gap, stated plainly: all four scenarios store the `Ok` variant. The
  idle-connection scenario does not store an `Err`; the server's 100 ms
  preamble timeout fires first and its reply code 3 is read successfully within
  the client's one-second budget. The `Err` variant is therefore reachable only
  when connect, write, or read fails outright, and no scenario exercises it.
  Its handling is covered by `reply_code`'s "missing reply" guard, which the
  scenarios reach only if the handshake step never stores anything.
- INV-3: dropping the world signals the server to shut down without awaiting
  it. Method: the scenario runtimes are current-thread, so a dropped world
  before the runtime drains would surface as a hung or leaked server; the suite
  completing in roughly the same wall-clock as the baseline is the evidence.
  This is an axiom about the runtime, not a library guarantee available for
  proof; the observable check is that the suite still terminates promptly.
- INV-4: the async conversion does not change the asserted protocol semantics.
  Method: exact reply-code equality against the unchanged feature file.

Lemma: the step wrapper's borrow of the world does not outlive a step's
`.await`. Method: steps request exactly one fixture and the scenario body is
empty, so no nested borrow is expressible; the compile is the discharge.
Non-vacuity: the same shapes that would violate this are the documented `E0499`/
`E0502` cases in the migration guide, which this file avoids by construction.

Axioms:

- rstest-bdd 0.6.0 awaits async step bodies under an async scenario and
  borrows fixtures mutably for `&mut` parameters. Evidence: its published
  `tests/async_step_functions.rs`, which exercises exactly this shape, plus the
  focused run of the handshake scenarios.
- Tokio's `current_thread` runtime drives tasks spawned with `tokio::spawn`
  while the test body is awaiting. Evidence: the pre-existing
  `tests/wireframe_handshake_metadata.rs` and the incumbent BDD tests rely on
  the same behaviour.
- The `wireframe` server's preamble timeout fires at the configured duration.
  Evidence: the pre-existing `replies_timeout_for_idle_socket` unit test in
  `src/wireframe/handshake.rs` asserts the timeout reply code against a 100 ms
  duration.

Negative control (discharged 2026-10-13): the asserted code in `then_code` was
temporarily replaced by `code.wrapping_add(99)` and the four scenarios were
re-run. All four failed, each reporting the reply code actually observed —
`left: 0` for "Successful handshake replies OK", and `left: 3` for "Idle
connection times out during handshake" — against the feature's own step text.
This discharges both INV-1 and INV-4: the scenarios do observe real reply codes
from the world, and the async conversion did not silently preserve a wrong
value. The mutation was then reverted and the suite re-run green.

No new invariant over generated inputs, orderings, or arithmetic is introduced:
the change is a mechanical restructuring of test scaffolding, so property
tests, model checking, and proofs add no rigour here and are not planned. The
strongest available evidence is the behavioural suite, which this plan
strengthens rather than weakens by keeping every assertion and its error
messaging intact.

Hosted review (2026-10-13, head `8bf07ec`): CodeRabbit requested changes with
two findings, both on this plan's prose rather than on the code. The first was
correct and substantive: INV-1 had claimed the idle-connection scenario stores
an *error* reply and so proves the error branch is exercised. Tracing
`reply_code` through `perform_handshake` shows all four scenarios store the
`Ok` variant — the server's 100 ms preamble timeout fires before the client's
one-second budget expires, so the client reads reply code 3 successfully. The
claim was wrong and is now replaced by an explicit statement of the residual
gap. The second finding, first-person wording at two sites, was a style-guide
violation and was fixed with impersonal phrasing. Neither finding touched
`src/`.

The first gate run supplies a useful bound on what that suite can discharge.
All eight tests passed while `start_server` carried a needless `async` marker;
only the denied `unused_async` lint exposed it. Behavioural tests establish
that the code does the right thing, not that each signature is honest about
what it does. Recording this keeps the plan from treating a green suite as
sufficient evidence for the shape of the change as well as its behaviour.

Independent inspection (2026-10-13): the local `coderabbit review --agent` pass
over the committed diff against `origin/main` (`df48e51`) inspected all three
changed files and reported zero findings. This is a second, independent
inspector over the same candidate, complementing rather than repeating the
behavioural suite. A hosted review covering the same head was queued
separately; its result is reported when it lands.

## Idempotence and recovery

Every step is a source edit or a read-only gate run, so re-running is safe. If
EP-M2 fails to compile because of the documented `&mut` fixture caveat, apply
the recorded fallback: keep the async conversion, revert step parameters to
`&HandshakeWorld`, and restore minimal interior mutability for the mutated
fields, documenting the ICE on the world type. `git revert` of the single
milestone commit returns the tree to the green baseline.

## Artefacts and notes

Baseline focused run (before any edit), captured at
`/tmp/baseline-issue-235-simplify-bdd-handshakeworld-implementation-in-handshake-tests.out`:

```plaintext
running 8 tests
test wireframe::handshake::tests::replies_handshake_errors::case_1 ... ok
test wireframe::handshake::tests::replies_handshake_errors::case_2 ... ok
test wireframe::handshake::tests::replies_success ... ok
test wireframe::handshake::bdd::replies_ok ... ok
test wireframe::handshake::bdd::unsupported_version ... ok
test wireframe::handshake::bdd::invalid_protocol ... ok
test wireframe::handshake::tests::replies_timeout_for_idle_socket ... ok
test wireframe::handshake::bdd::handshake_timeout ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 368 filtered out
```

## Interfaces and dependencies

After the change, `src/wireframe/handshake_bdd.rs` declares:

```rust,ignore
struct HandshakeWorld {
    addr: Option<SocketAddr>,
    shutdown: Option<oneshot::Sender<()>>,
    reply: Option<Result<[u8; REPLY_LEN], String>>,
}

impl HandshakeWorld {
    fn start_server(&mut self);
    async fn connect_and_maybe_send(&mut self, bytes: Option<Vec<u8>>)
        -> Result<(), String>;
    fn reply_code(&self) -> Result<u32, String>;
}

impl Drop for HandshakeWorld { fn drop(&mut self); }
```

No dependency changes. rstest-bdd 0.6.0, rstest 0.27.0, and Tokio 1 already
provide every facility used.

## Revision note

Initial draft, 2026-10-13: created alongside the implementation so the issue's
stale line citation, the already-satisfied 0.6.0 dependency, and the `&mut`
fixture ICE risk are all recorded before edits begin.

Second revision, 2026-10-13: the first gate run rejected an over-eager `async`
on `start_server`, so the async boundary moved to sit at the socket I/O and
every affected description was corrected. Gate results, the CodeRabbit review
outcome, and the `test-rstest-bdd-ui` timeout diagnosis were added, and the
plan was marked complete.

[issue-235]: https://github.com/leynos/mxd/issues/235
[pr-234]: https://github.com/leynos/mxd/pull/234
[pr-234-comment]: https://github.com/leynos/mxd/pull/234#discussion_r2576844667
