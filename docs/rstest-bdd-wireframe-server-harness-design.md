# `rstest-bdd` Wireframe server harness design

Status: Proposed companion design

Audience: developers implementing mxd behavioural tests, reviewers of the
`rstest-bdd` harness contract, and maintainers of `wireframe_testing`.

Scope: design an in-process harness for behavioural tests of the mxd Wireframe
server using `rstest-bdd` v0.6.0-beta3. This document does not implement the
harness or change production server behaviour.

Companion documents: [mxd design](design.md),
[verification strategy](verification-strategy.md),
[rstest-bdd user's guide](rstest-bdd-users-guide.md), and
[Wireframe user's guide](wireframe-users-guide.md).

## 1. Problem and context

The current Wireframe behavioural suites launch the
`mxd-wireframe-server` executable, release an ephemeral port before the child
binds it, poll for readiness, and use blocking `TcpStream` helpers. This gives
valuable black-box coverage, but every scenario pays for process startup,
contains a port-allocation race, and reports failures far from the server task
that caused them.

The bootstrap BDD module under `src/server/wireframe/` only validates bind
address parsing and preparation. It does not run the server, exchange the
Hotline preamble, route transactions, or exercise cleanup.

`rstest-bdd` v0.6.0-beta3 provides `HarnessAdapter`, an associated context,
`ScenarioRunRequest`, `ScenarioMetadata`, and the reserved
`rstest_bdd_harness_context` fixture key. The Skyjoust Bevy design supplies the
structural precedent: a `Default` adapter creates scenario-owned state, injects
a cloneable context, and cleans up after success or panic.

mxd additionally needs a continuously driven Tokio runtime, selectable database
fixtures, a Hotline preamble, `HotlineFrameCodec`, and a real loopback
connection through framing, middleware, routing, and session state.

## 2. Goals and non-goals

### Goals

- Add a project-specific `MxdWireframeHarness` in `test-util`.
- Give each scenario a dedicated current-thread Tokio runtime and `LocalSet`.
- Inject `MxdWireframeScenario` through the reserved context fixture.
- Let a `Given` step seed the database before starting the server.
- Bind a listener before spawning the server, removing the released-port race.
- Run the production application factory in-process with one server worker.
- Use a typed Hotline client backed by `HotlineCodec`.
- Preserve feature path, scenario name, and line in diagnostics.
- Clean up client, server, runtime thread, and database after success and panic.
- Reuse `wireframe_testing` wherever its public API matches mxd.
- Retain a narrow subprocess suite for binary and CLI wiring.

### Non-goals

- Do not implement the harness in this design-only change.
- Do not replace codec, fuzz, Kani, validator, or black-box coverage.
- Do not create a speculative generic Wireframe harness crate.
- Do not duplicate the production route and middleware graph in `test-util`.
- Do not enable global observability capture for every scenario.

## 3. Constraints in `rstest-bdd` v0.6.0-beta3

The adapter contract is synchronous. Scenario macros reject an `async fn`
scenario when `harness = ...` is present. An async step called while a harness
runtime is active receives only one poll; a network operation that returns
`Pending` therefore fails instead of reaching completion.

This rules out the tempting design where `MxdWireframeHarness::run` enters a
Tokio runtime and then invokes `request.run(context)`. Hotline handshake,
database work, and transaction exchange all need multi-poll futures.

The harness should instead run the server runtime on a dedicated operating
system thread. The scenario runner remains outside any Tokio runtime and talks
to that thread through commands. Existing step definitions can stay
synchronous, while the driver performs asynchronous work to completion.

A second beta3 limitation affects failure reporting. `HarnessError` only has a
runtime-construction variant. Server startup errors can remain ordinary step
errors because startup is lazy, but cleanup errors cannot be represented
faithfully through `HarnessResult`. Section 7 defines the temporary policy.

## 4. Design decision

The first implementation belongs under `test-util`, not in a new
`rstest-bdd-harness-wireframe` workspace crate.

A useful mxd scenario needs a Hotline preamble and codec, mxd database fixtures,
mxd configuration, session state, and mxd transaction helpers. Hiding those
behind associated types would create a nominally generic crate whose only
useful implementation still belongs entirely to mxd. That is abstraction by
fog machine.

The design separates two boundaries:

1. `mxd::server::wireframe::test_support` exposes a thin in-process start and
   shutdown seam using the production server builder.
2. `test_util::wireframe_harness` owns the `rstest-bdd` adapter, driver thread,
   database selection, Hotline client, and scenario command protocol.

Extraction should wait until `wireframe_testing` owns the generic server
lifecycle and a second application validates the API.

## 5. Architecture

The public API centres on `MxdWireframeHarness`, its cloneable
`MxdWireframeScenario` context, `ClientHandshake`, `StartOutcome`, and
`MxdHarnessError`.

`MxdWireframeHarness` implements `HarnessAdapter` with
`type Context = MxdWireframeScenario`. Its `run` method starts one
`ScenarioDriver` thread before invoking the scenario runner. The driver owns:

- a current-thread Tokio runtime and `LocalSet`;
- mutable scenario state;
- the test database guard and pool;
- the in-process server handle;
- the typed Hotline client; and
- the last decoded reply.

The context contains `ScenarioMetadata` and a cloneable command sender. Steps do
not borrow server or client objects directly. Each method sends one command and
waits for a typed reply under a command timeout.

```text
Scenario test thread                   Scenario driver thread
--------------------                   ----------------------
request.run(context)                   Tokio runtime + LocalSet
        |                                      |
        +---- Start(setup) ------------------->| build database and server
        <---- StartOutcome --------------------+
        +---- Exchange(transaction) ---------->| send, receive, decode
        <---- HotlineTransaction --------------+
        +---- Shutdown ------------------------>| stop server and release DB
        <---- Result ---------------------------+
```

_Figure 1: The synchronous `rstest-bdd` runner communicates with a dedicated
asynchronous scenario driver. The runner never enters the driver's runtime._

The private state machine moves from `Created` to `Running`, `Unavailable`, or
`Failed`; successful cleanup moves `Running` to `Stopped`. `Shutdown` is
idempotent. Commands invalid for the current state return
`MxdHarnessError::InvalidState`.

## 6. Scenario context contract

The first API should remain synchronous and task-oriented:

```rust
impl MxdWireframeScenario {
    pub fn metadata(&self) -> &ScenarioMetadata;
    pub fn set_handshake(&self, value: ClientHandshake) -> Result<(), MxdHarnessError>;
    pub fn start(&self, setup: SetupFn) -> Result<StartOutcome, MxdHarnessError>;
    pub fn exchange(&self, request: HotlineTransaction)
        -> Result<HotlineTransaction, MxdHarnessError>;
    pub fn last_reply(&self)
        -> Result<Option<HotlineTransaction>, MxdHarnessError>;
    pub fn database_pool(&self) -> Result<DbPool, MxdHarnessError>;
    pub fn reconnect(&self, value: ClientHandshake) -> Result<(), MxdHarnessError>;
    pub fn shutdown(&self) -> Result<(), MxdHarnessError>;
}
```

`set_handshake` is valid only before startup and supports sub-version
compatibility scenarios without process environment variables. `start` sends a
command that calls `build_test_db_async`, starts the server with that pool,
performs the preamble exchange, and stores a typed client.

`exchange` sends one request, awaits one reply under an operation timeout,
stores a clone as `last_reply`, and returns the owned reply. No command exposes
a mutex guard or a runtime-bound resource to a step.

`StartOutcome` preserves optional PostgreSQL availability:

```rust
pub enum StartOutcome {
    Running,
    Unavailable { reason: String },
}
```

Shared steps must emit the reason once and then no-op. Silent success is
forbidden. A future dynamic-skip facility should replace this compatibility
shape.

The command channel can use `tokio::sync::mpsc::UnboundedSender`; each command
carries a bounded standard-library reply channel. `recv_timeout` gives a useful
failure when the driver panics or deadlocks. The driver remains the sole owner
of mutable state, so no cross-thread application locks are required.

## 7. Harness lifecycle and diagnostics

`MxdWireframeHarness::run` should:

1. clone `ScenarioMetadata`;
2. spawn the driver thread and await runtime readiness;
3. create `MxdWireframeScenario` from the command sender and metadata;
4. run `request.run(scenario.clone())` inside `catch_unwind`;
5. send `Shutdown` and join the driver thread regardless of outcome; and
6. return the value or resume the original panic with scenario context.

Runtime construction failure maps to
`HarnessError::RuntimeBuildFailed`. The driver startup handshake must complete
before the scenario runner receives the context.

Generated tests use the ordinary synchronous attribute policy:

```rust
scenarios!(
    "tests/features/wireframe_routing",
    harness = test_util::MxdWireframeHarness,
    attributes = rstest_bdd_harness::DefaultAttributePolicy,
);
```

Server behavioural steps should remain synchronous and call the context
methods. This avoids one runtime per async step and makes the driver boundary
obvious. An async façade may be added later without changing the command
protocol.

Shutdown closes the client, signals the server, awaits its task under a bounded
timeout, aborts only after timeout, drops the database guard, and then lets the
runtime thread exit. A driver-thread `Drop` fallback may abort resources, but it
must not replace explicit shutdown and join.

The panic path logs cleanup failures and resumes the original panic. On the
normal path, beta3 cannot return a semantic cleanup error through
`HarnessResult`; the harness should therefore panic with feature path, scenario
name, line, cleanup stage, and source error. Mislabeling it as a runtime build
failure would corrupt diagnostics.

## 8. Production server seam and client

The harness must use the production application graph. The server module should
extract common construction from `WireframeBootstrap::run` and expose a
feature-gated entry point:

```rust
#[cfg(feature = "test-support")]
pub mod test_support {
    pub async fn spawn_server(
        config: Arc<AppConfig>,
        pool: DbPool,
        listener: std::net::TcpListener,
    ) -> Result<RunningWireframeServer>;
}
```

`spawn_server` should build Argon2, fresh outbound and presence registries, the
production application factory, `HotlinePreamble`, and production handshake
hooks. It should force one worker, bind the existing listener, attach readiness
and shutdown channels, spawn the task, and await readiness before returning.

Production startup and test startup must call the same private server builder.
A separate test route graph would make the harness fast but untrustworthy.
`RunningWireframeServer` exposes only `local_addr` and async `shutdown`.

The driver owns a typed client:

```rust
struct HotlineTestClient {
    framed: Framed<TcpStream, HotlineCodec>,
}
```

Connection setup writes the 12-byte Hotline preamble, validates the 8-byte
reply, and then wraps the stream in `Framed`. Connect, send, and receive each
use explicit timeouts carrying scenario metadata.

Typed exchange covers current routing, login, file, news, fragmentation, and
XOR compatibility scenarios. A raw-byte client remains a follow-up for
malformed frames and partial writes; existing raw tests should remain until it
lands.

## 9. `wireframe_testing` reuse and gaps

Add `wireframe_testing = "0.3.0"` to `test-util` and reuse matching pieces.

| Capability | Decision | Rationale |
| --- | --- | --- |
| `unused_listener()` | Use | Binding before spawn removes the port race. |
| Codec and fragment drivers | Use in component tests | They avoid duplicated malformed byte fixtures. |
| `ObservabilityHandle` | Opt in | Global logger and thread-local metrics constrain parallel use. |
| `WireframePair` | Do not use yet | Its client and builders cannot represent Hotline. |
| Echo envelope and factory | Do not use | They test Wireframe, not mxd's application graph. |

_Table 1: `wireframe_testing` capabilities selected for the mxd harness._

The current pair harness has five blocking gaps:

1. `WireframePair` stores a concrete default `WireframeClient` using the
   length-delimited client codec; mxd needs `HotlineCodec`.
2. Its closure accepts and returns
   `WireframeClientBuilder<BincodeSerializer, (), ()>`. `with_preamble` changes
   the preamble type, so the closure cannot configure Hotline.
3. It offers no server configuration closure for `HotlinePreamble`, handshake
   hooks, worker count, or other application settings.
4. The pair is not generic over a caller-supplied client or async connector.
5. No server-only running handle can be combined with a protocol-specific
   client.

The smallest useful upstream extension is a server lifecycle primitive plus a
generic client connector:

```rust
pub struct RunningWireframeServer;

pub async fn spawn_wireframe_server_with<Start, Fut>(
    start: Start,
) -> TestResult<RunningWireframeServer>
where
    Start: FnOnce(ServerHarnessParts) -> Fut,
    Fut: Future<Output = TestResult<()>> + Send + 'static;

pub struct WireframePair<C> {
    pub server: RunningWireframeServer,
    pub client: C,
}
```

`ServerHarnessParts` should carry an already-bound listener, readiness sender,
and shutdown future. A second helper should accept an async client factory
receiving the server address. The existing pair helper can remain a
compatibility wrapper.

After that API ships, mxd should delete its local server lifecycle wrapper while
keeping the mxd-specific scenario, driver, and Hotline client.

## 10. Migration, layout, and verification

Setup steps request `MxdWireframeScenario` through
`#[from(rstest_bdd_harness_context)]` and call `start(setup_login_db)`.
Transaction steps build `HotlineTransaction` values and call `exchange`.
Assertions read `last_reply`. The wrapper worlds in
`wireframe_routing_bdd.rs`, `wireframe_login_compat.rs`, and
`wireframe_xor_compat.rs` then disappear.

The process-based `TestServer` remains for executable startup, CLI and
environment configuration, database URL propagation, process termination, and
one successful handshake and transaction.

```text
test-util/src/wireframe_harness/
|-- mod.rs
|-- client.rs
|-- command.rs
|-- context.rs
|-- driver.rs
|-- error.rs
|-- harness.rs
`-- panic.rs

src/server/wireframe/
|-- mod.rs
|-- runtime.rs
`-- test_support.rs
```

The workspace should pin the complete BDD family to the requested prerelease:

```toml
[workspace.dependencies]
rstest-bdd = "0.6.0-beta3"
rstest-bdd-harness = "0.6.0-beta3"
rstest-bdd-macros = {
    version = "0.6.0-beta3",
    features = ["compile-time-validation"],
}
```

`test-util` also needs `wireframe_testing`, Tokio networking, synchronization,
time, and I/O features, `tokio-util` with `codec`, and `futures-util`.

Unit coverage should verify metadata injection, driver readiness, state
transitions, handshake validation, command and operation timeouts, idempotent
shutdown, cleanup after success and panic, driver panic propagation, and
unavailable database reporting.

Behavioural migration should start with unknown transaction routing and login,
then add file, news, and sub-version/XOR scenarios. Parallel coverage should
prove distinct listeners, databases, registries, runtime threads, and sessions.
Observability scenarios remain serialized.

Macro coverage should prove a `Default` custom harness with `scenarios!`,
`DefaultAttributePolicy`, synchronous steps, and reserved context injection
under v0.6.0-beta3.

The implementation must pass:

```bash
make check-fmt
make lint
make test
cargo test --workspace --no-default-features --features sqlite,test-support
cargo test --workspace --no-default-features --features postgres,test-support
```

CI needs at least one lane where PostgreSQL scenarios actually run; an
unavailable local backend must remain visible in output.

## 11. Risks and rejected alternatives

A driver thread adds a command boundary. Typed commands and reply timeouts make
that boundary explicit, while sole ownership removes a larger tangle of
cross-thread locks and runtime re-entry hazards.

Cleanup must never replace the original step panic. Cleanup failures on that
path become structured tracing records with the same scenario metadata.

The test seam could drift from production. Shared private construction plus one
subprocess parity scenario prevents a quiet fork.

Wrapping the subprocess world in `HarnessAdapter` would preserve startup cost,
blocking I/O, the port race, and weak task diagnostics.

Entering a harness runtime around `request.run` would trigger beta3's one-poll
async-step limitation. Repeatedly calling `Runtime::block_on` from steps would
also risk nested-runtime panics as soon as an async step appears.

Starting the server before the runner would require metadata tags to select a
database fixture. Lazy startup preserves the readable `Given` contract and
avoids a hidden tag-to-function registry.

Generalizing now would require associated types for preamble, codec, client,
factory, database, configuration, and errors. That surface should emerge from
two working applications, not speculative type parameters.

A byte adapter cannot force `WireframePair` to fit because the mismatch occurs
at transport codec, preamble typestate, and server configuration boundaries.

## 12. Recommendation

Implement `MxdWireframeHarness` in `test-util` as a synchronous scenario façade
over a dedicated Tokio driver thread. Add the smallest feature-gated production
spawn seam that reuses the real application factory. Reuse
`wireframe_testing::unused_listener` immediately, and use its codec and
observability tools where their constraints fit. Propose a generic server-only
lifecycle and custom client connector upstream.

This removes the slowest and least diagnostic plumbing without pretending that
the current `WireframePair` speaks Hotline. When the upstream lifecycle API
matures, mxd can delete its thin local wrapper without redesigning Gherkin
steps, the driver protocol, or the `rstest-bdd` context contract.
