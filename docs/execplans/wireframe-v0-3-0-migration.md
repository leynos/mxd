# Adopt wireframe v0.3.0 in mxd

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

## Purpose / big picture

After this change, `mxd` builds, tests, and runs against `wireframe` v0.3.0
without relying on APIs removed since v0.2.0. The migration should also take
advantage of one v0.3.0 capability that clearly fits this codebase: a fallible
app factory for the wireframe server, so a connection that cannot build a valid
`WireframeApp` fails closed instead of falling back to a degraded default app.

Observable outcome: `Cargo.toml` and `Cargo.lock` resolve `wireframe` v0.3.0,
the wireframe-only test suite still passes, and the server preserves current
Hotline behaviour for handshake handling, transaction routing, XOR
compatibility, and outbound push delivery. If selected transport tests are
converted to `wireframe::testkit`, those tests should become simpler without
changing behaviour.

## Constraints

- Preserve the current Hotline wire contract, including the handshake reply,
  transaction framing, transaction reassembly, XOR compatibility, and route
  dispatch behaviour.
- Keep `src/wireframe/codec/mod.rs`, `src/wireframe/codec/framed.rs`, and
  `src/wireframe/codec/frame.rs` as the protocol authority for Hotline framing.
  This migration must not replace Hotline-specific framing with `wireframe`
  fragmentation or the message assembler in the same change.
- Do not introduce any new third-party crate unless the user explicitly
  approves it. Enabling new features on the existing `wireframe` dependency is
  allowed; adding the separate `wireframe_testing` crate is not.
- Keep public `mxd` APIs and the behaviour of the `mxd-wireframe-server`
  binary stable.
- Follow repository quality gates: `make check-fmt`, `make lint`, and
  `make test` must pass before the migration is considered complete.
- Keep Rust files under 400 lines and retain module-level `//!` comments.

## Tolerances (exception triggers)

- Scope: if the migration needs changes in more than 15 files or more than
  500 net lines, stop and review the plan before continuing.
- Interfaces: if the migration requires changing public `mxd` interfaces
  outside the wireframe integration modules, stop and escalate.
- Dependencies: if `wireframe::testkit` is insufficient and the work appears
  to require `wireframe_testing`, stop and get approval before adding it.
- Validation: if `make test-wireframe-only` or the equivalent focused suite
  still fails after two fix iterations, stop and review the failing surface.
- Ambiguity: if `wireframe` v0.3.0's fallible factory semantics do not map
  cleanly to the current handshake/app-context flow, stop and confirm the
  desired failure mode before proceeding.

## Risks

- Risk: changing the app factory from infallible to fallible may alter how
  connection setup failures surface. Severity: high. Likelihood: medium.
  Mitigation: add or update targeted tests around missing handshake context and
  successful handshake setup before removing the fallback path.

- Risk: root re-export removal may break doc examples, tests, or helper code
  beyond the three primary source files already identified. Severity: medium.
  Likelihood: medium. Mitigation: run a focused wireframe-only compile after
  the dependency bump, then fix import paths before broader refactors.

- Risk: `wireframe::testkit` may not fit tests that depend on the Hotline
  preamble or custom `HotlineCodec`. Severity: medium. Likelihood: medium.
  Mitigation: adopt it only in low-level codec/transport tests first, and keep
  the existing `src/wireframe/test_helpers/` fixtures for protocol-specific
  data.

- Risk: the migration may accidentally widen scope into unused client features
  such as pooling, streaming, or tracing. Severity: low. Likelihood: medium.
  Mitigation: explicitly treat those surfaces as out of scope unless a real
  `WireframeClient` use appears in the codebase.

## Progress

- [x] (2026-04-06) Review `docs/wireframe-v0-2-0-to-v0-3-0-migration-guide.md`
  and `docs/wireframe-users-guide.md`.
- [x] (2026-04-06) Inventory current `wireframe` usage in `Cargo.toml`,
  `src/server/wireframe/mod.rs`, `src/wireframe/protocol.rs`,
  `src/wireframe/outbound.rs`, `src/wireframe/codec/frame.rs`,
  `src/wireframe/handshake.rs`, and the wireframe-focused test files.
- [x] (2026-04-06) Identify the highest-value v0.3.0 adoption points:
  removed root re-exports, fallible app factories, and selective testkit
  adoption.
- [x] (2026-04-06) Update the `wireframe` dependency to v0.3.0 and regenerate
  `Cargo.lock`.
- [x] (2026-04-06) Fix compile-time breakage from removed root re-exports and
  changed trait bounds by moving imports to `wireframe::hooks`,
  `wireframe::session`, and `wireframe::serializer`, and by adapting
  `HotlineFrameCodec` to the v0.3.0 `FrameCodec::wrap_payload(&self, Bytes)`
  signature.
- [x] (2026-04-06) Refactor the wireframe server bootstrap to use a fallible
  app factory and remove the degraded fallback app path. The factory now
  returns a typed `AppFactoryError`, and unit tests assert that missing
  handshake context fails closed while valid context still builds an app.
- [x] (2026-04-06) Evaluate `wireframe::testkit` for targeted transport suites.
  Conclusion: no adoption in this change because the migration completed with
  focused compatibility fixes and the existing Hotline-specific helpers remain
  the better fit for the current preamble and framing tests.
- [x] (2026-04-06) Run the full repository validation suite and capture the
  final outcome in this ExecPlan. `make check-fmt`, `make lint`, and
  `make test` all pass after the migration.

## Surprises & discoveries

- Observation: `mxd` already avoids several v0.3.0 breakages.
  Evidence: current code does not use `WireframeApp::new()`, `AppDataStore`,
  `WireframeClient`, `PacketParts::payload`, `FragmentParts::payload`, or
  `BackoffConfig::normalised`. Impact: the migration can stay focused on a
  narrow set of concrete changes.

- Observation: `src/server/wireframe/mod.rs` initially validated app
  construction at startup, then logged and fell back to `HotlineApp::default()`
  when per-connection app setup failed. Evidence: `build_app_with_logging()`
  caught `try_build_app()` errors and returned `fallback_app()`. Impact:
  v0.3.0's fallible app factory directly addressed an existing workaround and
  was the clearest capability win for `mxd`.

- Observation: Hotline transaction fragmentation is already implemented inside
  `HotlineCodec`, not by wireframe's transport fragmentation layer. Evidence:
  `src/wireframe/codec/framed.rs` reassembles incoming fragments and fragments
  large outbound payloads itself. Impact: `memory_budgets`,
  `enable_fragmentation()`, and `with_message_assembler()` are not direct
  drop-ins for this migration and should be deferred unless the protocol layer
  is redesigned.

- Observation: the codebase has no current `WireframeClient` usage.
  Evidence: the wireframe integration is concentrated in server bootstrap,
  protocol hooks, codecs, and server-side tests. Impact: client pooling,
  streaming, tracing, and request hooks are presently informative but not
  applicable migration targets.

- Observation: `wireframe` v0.3.0's fallible app factory fits the existing
  handshake-context dependency cleanly. Evidence: replacing `fallback_app()`
  with a `Result<HotlineApp, AppFactoryError>` closure required only local
  changes in `src/server/wireframe/mod.rs` plus targeted tests. Impact: the
  server now fails closed when per-connection handshake context is missing,
  without widening the migration into route or middleware redesign.

- Observation: `wireframe::testkit` is not a clear win for the current Hotline
  transport tests. Evidence: the suites most relevant to the migration already
  depend on Hotline-specific preamble bytes, frame reassembly, and bespoke
  helpers, and the dependency bump plus app-factory refactor did not create
  painful test boilerplate. Impact: the migration can stay smaller and lower
  risk by keeping the existing test helpers for now.

## Decision log

- Decision: scope the migration around the existing server runtime rather than
  unused client APIs. Rationale: no current `WireframeClient` usage exists, so
  client pooling, streaming, tracing, and request hooks would add new surface
  without solving a present problem. Date/Author: 2026-04-06 / Assistant

- Decision: treat the fallible app factory as the primary v0.3.0 capability to
  adopt in code. Rationale: it removes the current degraded fallback path in
  `src/server/wireframe/mod.rs` and aligns failure handling with the existing
  `try_build_app()` split. Date/Author: 2026-04-06 / Assistant

- Decision: keep Hotline framing and reassembly in the in-tree codec modules.
  Rationale: replacing them with wireframe fragmentation or the message
  assembler would combine a version bump with a protocol redesign, which is
  unnecessary risk for this migration. Date/Author: 2026-04-06 / Assistant

- Decision: consider `wireframe::testkit` only for targeted low-level suites,
  and defer `wireframe_testing` unless approval is given. Rationale:
  feature-only adoption is lower risk than adding a new crate, and the custom
  Hotline preamble means not every v0.3.0 test helper will fit. Date/Author:
  2026-04-06 / Assistant

- Decision: keep the existing Hotline-specific transport helpers instead of
  adopting `wireframe::testkit` in this migration. Rationale: after the
  dependency bump and focused compatibility fixes, `testkit` did not offer a
  meaningful simplification for the current preamble and codec suites.
  Date/Author: 2026-04-06 / Assistant

## Outcomes & retrospective

The migration landed as a narrow compatibility change: `wireframe` now resolves
to v0.3.0, import-path and trait-signature breakage is repaired, and
per-connection app construction now fails closed instead of silently falling
back to `HotlineApp::default()`. Focused wireframe-only checks passed during
development, and the full repository gates (`make check-fmt`, `make lint`, and
`make test`) passed at the end. `wireframe::testkit` was evaluated but not
adopted because the existing Hotline-specific helpers remain the better fit for
the current preamble and framing suites.

## Context and orientation

`mxd` is a Rust workspace whose wireframe integration lives under
`src/server/wireframe/` and `src/wireframe/`. The manifest at `Cargo.toml` now
declares `wireframe = "0.3.0"` and builds the `mxd-wireframe-server` binary
from `src/bin/mxd_wireframe_server.rs`.

The key runtime file is `src/server/wireframe/mod.rs`. It defines `HotlineApp`
as `WireframeApp<BincodeSerializer, (), Envelope, HotlineFrameCodec>`, prepares
a `WireframeServer`, installs Hotline preamble hooks from
`src/wireframe/handshake.rs`, disables wireframe fragmentation with
`.fragmentation(None)`, and registers the Hotline protocol and transaction
middleware.

`src/wireframe/protocol.rs` implements wireframe protocol hooks for Hotline. It
now imports `ConnectionContext` and `WireframeProtocol` from `wireframe::hooks`.

`src/wireframe/outbound.rs` stores and retrieves per-connection push handles.
It now imports `ConnectionId` and `SessionRegistry` from `wireframe::session`.

`src/wireframe/codec/frame.rs` adapts the in-tree `HotlineCodec` to wireframe's
`FrameCodec` trait. It is the bridge between Hotline transaction bytes and
wireframe `Envelope` payloads.

`src/wireframe/handshake.rs` attaches preamble success and failure handlers,
stores handshake context, and contains tests that now import
`BincodeSerializer` from `wireframe::serializer`.

The most relevant tests for this migration are:

1. `tests/wireframe_handshake_metadata.rs`
2. `tests/wireframe_transaction.rs`
3. `tests/wireframe_xor_compat.rs`
4. `src/wireframe/codec/tests.rs`
5. `src/wireframe/codec/framed_tests.rs`
6. `src/wireframe/handshake.rs` tests

In this plan, a "fallible app factory" means a closure passed to
`WireframeServer::new` that returns `Result<WireframeApp, E>` instead of always
returning an app directly. Wireframe v0.3.0 can propagate that error without
forcing `mxd` to return a default app that accepts traffic without the required
per-connection routing state.

## Plan of work

### Stage A: bump the dependency and surface actual breakage

Update `Cargo.toml` from `wireframe = "0.2.0"` to `wireframe = "0.3.0"`, then
refresh `Cargo.lock`. Do not enable new dependency features yet, because the
first goal is to let the compiler reveal which existing imports and trait
usages actually break.

After the version bump, run a focused wireframe-only compile first. This keeps
the feedback loop short and avoids burying API breakage under unrelated
backend-specific output.

### Stage B: repair v0.3.0 breaking changes already used by mxd

Fix the removed root re-export usages first:

1. In `src/wireframe/protocol.rs`, import
   `ConnectionContext` and `WireframeProtocol` from `wireframe::hooks`.
2. In `src/wireframe/outbound.rs`, import `ConnectionId` and
   `SessionRegistry` from `wireframe::session`.
3. In `src/wireframe/handshake.rs` test code and any similar sites, import
   `BincodeSerializer` from `wireframe::serializer`.

While doing this, inspect compile errors for any secondary breakage in tests,
doc examples, or helper modules. Because the codebase does not appear to use
other removed or renamed surfaces, this stage should remain small.

### Stage C: adopt the fallible factory path

Refactor `src/server/wireframe/mod.rs` so the closure passed to
`WireframeServer::new` returns `Result<HotlineApp, AppFactoryError>` rather
than always returning `HotlineApp`.

The target shape is:

```rust
let app_factory = {
    let pool = pool.clone();
    let argon2 = Arc::clone(&argon2);
    let outbound_registry = Arc::clone(&outbound_registry);
    move || try_build_app(&pool, &argon2, &outbound_registry)
};
```

Then remove the fallback path that currently logs and returns
`HotlineApp::default()`. That default app is safe as a compile-time type, but
unsafe as a runtime behaviour because it can accept traffic without the
handshake-derived routing context that `mxd` expects.

Keep `validate_app_factory()` if it still provides value as a startup-time
sanity check for route registration and middleware wiring. The difference is
that startup validation remains explicit, while per-connection context failures
must propagate rather than silently downgrading.

Before considering this stage done, add or update a test that proves the app
factory fails closed when handshake context is missing or incomplete, and that
the valid handshake path still sets up the app normally.

### Stage D: selectively adopt v0.3.0 test helpers

Once the runtime compiles and the focused tests are green, review whether one
or two low-level suites become simpler with `wireframe::testkit`. The best
starting points are `src/wireframe/codec/tests.rs` and
`src/wireframe/handshake.rs` tests, because they already exercise transport
edges such as partial frames, fragmented payloads, or slow reads.

The goal is not to replace `src/wireframe/test_helpers/`. Those helpers encode
Hotline-specific knowledge that `wireframe::testkit` does not provide. The goal
is only to replace bespoke transport driving where v0.3.0 now offers a better
primitive.

If `testkit` clearly improves a test, add the minimal dependency feature
required for test builds and migrate that suite. If it does not, record the
discovery in this ExecPlan and keep the existing helpers.

### Stage E: full validation and closeout

After all code changes land, run the full project gates. The migration is only
done when formatting, linting, and all test suites pass. Capture the final
results and any deviations in this ExecPlan before marking it complete.

## Concrete steps

All commands run from:

```plaintext
/data/leynos/Projects/mxd.worktrees/wireframe-v0-3-0-migration
```

1. Bump the dependency and refresh the lockfile.

   ```plaintext
   cargo update -p wireframe --precise 0.3.0
   ```

   Expected transcript excerpt:

   ```plaintext
   Updating wireframe v0.2.0 -> v0.3.0
   ```

2. Run a focused wireframe-only compile or test pass to surface API errors
   quickly.

   ```plaintext
   make typecheck-wireframe-only
   make test-wireframe-only
   ```

   Expected outcome before fixes: compile errors or test failures that mention
   moved imports such as `ConnectionContext`, `WireframeProtocol`,
   `ConnectionId`, `SessionRegistry`, or `BincodeSerializer`.

3. After import fixes, re-run the focused wireframe-only suite.

   ```plaintext
   make typecheck-wireframe-only
   make test-wireframe-only
   ```

   Expected outcome after fixes: the wireframe-only build succeeds and the
   focused tests pass.

4. After the fallible factory refactor and any targeted test updates, run the
   full repository gates.

   ```plaintext
   make check-fmt
   make lint
   make test
   ```

   Expected outcome:

   ```plaintext
   cargo fmt --all -- --check
   ...
   Finished `dev` profile ...
   ...
   test result: ok
   ```

5. If implementation edits any Markdown beyond this ExecPlan, also run:

   ```plaintext
   make fmt
   make markdownlint
   ```

## Validation and acceptance

The migration is accepted when all of the following are true:

- `Cargo.toml` and `Cargo.lock` resolve `wireframe` v0.3.0.
- No code still depends on removed root re-exports for
  `ConnectionContext`, `WireframeProtocol`, `ConnectionId`, `SessionRegistry`,
  or `BincodeSerializer`.
- `mxd-wireframe-server` still handles valid and invalid Hotline handshakes as
  before, proven by the existing handshake-focused tests.
- Transaction routing, XOR compatibility, and outbound push behaviour still
  pass the current wireframe-focused test suites.
- The server no longer falls back to `HotlineApp::default()` when
  per-connection app construction fails; instead, that failure is surfaced and
  the connection fails closed.
- If `wireframe::testkit` is adopted, at least one migrated suite proves the
  new helper is a net simplification without changing observable behaviour.

Quality criteria:

- Tests: `make test`
- Formatting: `make check-fmt`
- Linting: `make lint`
- Migration-focused regression check: `make test-wireframe-only`

## Idempotence and recovery

The dependency bump, import fixes, and validation commands are all safe to run
multiple times. If the v0.3.0 bump exposes unexpected breakage, revert only the
manifest and lockfile changes first, then re-apply the migration in smaller
steps.

If `wireframe::testkit` adoption broadens scope without clear payoff, revert
that sub-step independently and complete the core version bump first. The
capability adoption is optional; the compatibility migration is not.

## Artifacts and notes

Useful code snippets to preserve during implementation:

```rust
use wireframe::hooks::{ConnectionContext, WireframeProtocol};
use wireframe::session::{ConnectionId, SessionRegistry};
use wireframe::serializer::BincodeSerializer;
```

Relevant files for review during implementation:

```plaintext
Cargo.toml
Cargo.lock
src/server/wireframe/mod.rs
src/wireframe/protocol.rs
src/wireframe/outbound.rs
src/wireframe/handshake.rs
src/wireframe/codec/frame.rs
src/wireframe/codec/tests.rs
tests/wireframe_handshake_metadata.rs
tests/wireframe_transaction.rs
tests/wireframe_xor_compat.rs
```

## Interfaces and dependencies

The runtime should continue to expose the same `HotlineApp` alias in
`src/server/wireframe/mod.rs`:

```rust
type HotlineApp =
    WireframeApp<BincodeSerializer, (), Envelope, HotlineFrameCodec>;
```

The `HotlineProtocol` type in `src/wireframe/protocol.rs` should continue to
implement wireframe protocol hooks, but through the v0.3.0 module path:

```rust
impl WireframeProtocol for HotlineProtocol {
    type Frame = Vec<u8>;
    type ProtocolError = ();
}
```

The dependency plan is:

1. Update the main dependency to `wireframe = "0.3.0"`.
2. Add only the `testkit` feature if a migrated test proves it is useful.
3. Do not add `wireframe_testing` without explicit approval.

## Revision note

Created this draft after reviewing the current `mxd` wireframe integration and
the v0.3.0 migration/user guides. The draft narrows the migration to import
surface repairs, a fallible app factory, and optional `wireframe::testkit`
adoption, because those are the only changes that currently appear both
applicable and valuable.
