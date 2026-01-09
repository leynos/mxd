# Execution plan: Model handshake readiness in TLA+

**Task:** 1.2.4 from `docs/roadmap.md` **Branch:**
`1-2-4-model-handshake-readiness` **Status:** Complete

## Objective

Model the mxd handshake state machine in Temporal Logic of Actions (TLA+) and
verify invariants using TLC (the TLA+ model checker). The spec must demonstrate
no violations for bounded client counts and document timeout, error-code, and
readiness invariants.

## Acceptance criteria

From the roadmap:

> TLC runs the `crates/mxd-verification/tla/MxdHandshake.tla` spec with no
> invariant violations for bounded client counts and documents timeout,
> error-code, and readiness invariants.

## Background context

### Handshake protocol (from `src/protocol.rs` and `docs/protocol.md`)

- **Client sends 12 bytes:** Protocol ID ("TRTP", 4 bytes) + Sub-protocol
  (4 bytes) + Version (2 bytes) + Sub-version (2 bytes)
- **Server replies 8 bytes:** Protocol ID ("TRTP", 4 bytes) + Error code
  (4 bytes)
- **Error codes:**
  - `0` = `HANDSHAKE_OK` (success)
  - `1` = `HANDSHAKE_ERR_INVALID` (invalid protocol ID)
  - `2` = `HANDSHAKE_ERR_UNSUPPORTED_VERSION` (bad version)
  - `3` = `HANDSHAKE_ERR_TIMEOUT` (5-second timeout)
- **Timeout:** 5 seconds (`HANDSHAKE_TIMEOUT`)

### Verification strategy (from `docs/verification-strategy.md`)

- TLA+ specs live under `crates/mxd-verification/tla/`
- Each spec has a `.tla` and matching `.cfg` file
- TLC is run locally via `tlc2.TLC -config <spec>.cfg <spec>.tla`
- The `crates/mxd-verification/` directory does not yet exist

### Key source files

- `src/protocol.rs:14-35` — Handshake constants and error codes
- `src/wireframe/handshake.rs` — Handshake hook installation
- `src/wireframe/preamble.rs` — Preamble validation
- `docs/verification-strategy.md` — Three-tier verification approach

## Implementation steps

### Step 1: Create the verification crate structure

Create the `crates/mxd-verification/` workspace member with minimal scaffolding.

**Files to create:**

```text
crates/
  mxd-verification/
    Cargo.toml
    src/
      lib.rs
    tla/
      .gitkeep
```

**Cargo.toml contents:**

```toml
[package]
name = "mxd-verification"
version = "0.1.0"
edition = "2024"
description = "Formal verification specs and harnesses for mxd"

[dependencies]

[dev-dependencies]
rstest = "0.26"

[lints]
workspace = true
```

**lib.rs contents:**

```rust
//! Formal verification specifications and test harnesses for mxd.
//!
//! This crate contains TLA+ specifications, Stateright models, and Kani
//! harnesses that verify correctness-critical behaviour of the mxd server.
//! See `docs/verification-strategy.md` for the verification approach.
```

**Update root Cargo.toml:**

Add `"crates/mxd-verification"` to the workspace members list.

**Commit:** "Add mxd-verification crate skeleton"

______________________________________________________________________

### Step 2: Write the TLA+ handshake specification

Create `crates/mxd-verification/tla/MxdHandshake.tla` modeling the server-side
handshake state machine.

**State space design:**

- **Constants:**
  - `MaxClients` — Bounded number of concurrent clients (e.g., 3)
  - `TimeoutTicks` — Discrete time steps before timeout (e.g., 5)

- **Variables (per client):**
  - `state` — Connection state: `"Idle"`, `"AwaitingHandshake"`,
    `"Validating"`, `"Ready"`, `"Error"`
  - `errorCode` — Reply code: 0–3
  - `ticksElapsed` — Elapsed time ticks
  - `protocolValid` — Whether client sent valid protocol ID
  - `versionSupported` — Whether client sent supported version

- **Actions:**
  - `ClientConnect(c)` — Transition Idle → AwaitingHandshake
  - `ReceiveHandshake(c, valid, supported)` — Receive handshake bytes
  - `Validate(c)` — Check protocol and version, set error code, transition to
    Ready or Error (reply merged into this action)
  - `Tick` — Increment elapsed ticks; atomically transition to Error on timeout
  - `ClientDisconnect(c)` — Return to Idle

- **Invariants:**
  - `TypeInvariant` — All variables have correct types
  - `TimeoutInvariant` — Clients in AwaitingHandshake have ticksElapsed <
    TimeoutTicks
  - `ErrorCodeInvariant` — Error codes match validation failures
  - `ReadinessInvariant` — Ready ⇒ errorCode = 0 ∧ valid protocol ∧ supported
    version
  - `NoReadyWithError` — Ready and Error are mutually exclusive

**Commit:** "Add TLA+ handshake specification"

______________________________________________________________________

### Step 3: Write the TLC configuration

Create `crates/mxd-verification/tla/MxdHandshake.cfg` with model bounds.

**Configuration:**

```text
SPECIFICATION Spec

CONSTANTS
    MaxClients = 3
    TimeoutTicks = 5

INVARIANTS
    TypeInvariant
    TimeoutInvariant
    ErrorCodeInvariant
    ReadinessInvariant
    NoReadyWithError
```

**Trade-off:** `MaxClients = 3` keeps state space tractable (~10⁶ states) while
exercising concurrency. Increase for deeper exploration if needed.

**Commit:** "Add TLC configuration for handshake spec"

______________________________________________________________________

### Step 4: Create Docker wrapper for TLA+ tools

Create `scripts/run-tlc.sh` that bundles TLA+ tools in Docker, avoiding local
installation requirements.

**Script structure:**

```bash
#!/usr/bin/env bash
# Run TLC model checker via Docker
# Usage: ./scripts/run-tlc.sh <spec.tla> [spec.cfg]

set -euo pipefail

# TLC_IMAGE must be set by the caller (Makefile or CI workflow)
if [[ -z "${TLC_IMAGE:-}" ]]; then
    echo "Error: TLC_IMAGE environment variable must be set" >&2
    echo "Use 'make tlc-handshake' or set TLC_IMAGE explicitly" >&2
    exit 1
fi
TLC_WORKERS="${TLC_WORKERS:-auto}"

if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <spec.tla> [spec.cfg]" >&2
    exit 1
fi

SPEC_FILE="$1"
CFG_FILE="${2:-${SPEC_FILE%.tla}.cfg}"

# Verify files exist
if [[ ! -f "$SPEC_FILE" ]]; then
    echo "Error: Specification file not found: $SPEC_FILE" >&2
    exit 1
fi

if [[ ! -f "$CFG_FILE" ]]; then
    echo "Error: Configuration file not found: $CFG_FILE" >&2
    exit 1
fi

exec docker run --rm \
    -v "$(pwd):/workspace" \
    --workdir /workspace \
    "$TLC_IMAGE" \
    -workers "$TLC_WORKERS" -config "$CFG_FILE" "$SPEC_FILE"
```

**Commit:** "Add Docker wrapper for TLA+ tools"

______________________________________________________________________

### Step 5: Add Makefile targets for TLA+ verification

Add targets to run TLC on the handshake spec via Docker.

**New targets:**

```makefile
.PHONY: tlc tlc-handshake

TLC_RUNNER ?= ./scripts/run-tlc.sh

tlc: tlc-handshake ## Run all TLA+ model checks

tlc-handshake: ## Run TLC on handshake spec
	$(TLC_RUNNER) crates/mxd-verification/tla/MxdHandshake.tla
```

**Commit:** "Add Makefile targets for TLA+ verification"

______________________________________________________________________

### Step 6: Add integration test for TLC execution

Create `crates/mxd-verification/tests/tlc_handshake.rs` that programmatically
invokes TLC via the Docker wrapper and asserts no violations.

**Test structure:**

```rust
//! Integration test validating TLC runs the handshake spec without violations.

use std::process::Command;

/// Run TLC on the handshake spec and verify no invariant violations.
///
/// This test requires Docker to be available. Run with:
/// `cargo test -p mxd-verification -- --ignored` or `make tlc`.
#[test]
#[ignore]
fn tlc_handshake_no_violations() {
    // 1. Check Docker is available
    // 2. Run scripts/run-tlc.sh via std::process::Command
    // 3. Assert exit code 0
    // 4. Assert stdout contains "Model checking completed. No error has been found."
    // 5. If violation found, print counterexample trace
}
```

**Commit:** "Add TLC integration test for handshake spec"

______________________________________________________________________

### Step 7: Add GitHub Actions workflow for TLA+ verification

Create `.github/workflows/tlc.yml` to run TLC on every PR.

**Workflow structure:**

```yaml
name: TLA+ Verification

on:
  push:
    branches: [main]
    paths:
      - 'crates/mxd-verification/tla/**'
      - 'scripts/run-tlc.sh'
  pull_request:
    paths:
      - 'crates/mxd-verification/tla/**'
      - 'scripts/run-tlc.sh'

jobs:
  tlc:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run TLC handshake verification
        run: make tlc-handshake

      - name: Upload counterexample on failure
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: tlc-counterexample
          path: '*.out'
```

**Commit:** "Add GitHub Actions workflow for TLA+ verification"

______________________________________________________________________

### Step 8: Update documentation

**docs/verification-strategy.md:**

Add a section documenting the handshake spec:

```markdown
### Handshake specification (MxdHandshake.tla)

The handshake spec models the server-side state machine for client connections.
It verifies:

- Timeout behaviour fires after 5 seconds of inactivity
- Error codes correctly map to validation failures
- Ready state is only reachable with valid protocol and version
- States progress monotonically (no regression from terminal states)

Run locally with:

    make tlc-handshake
```

**docs/roadmap.md:**

Mark task 1.2.4 as complete with status note.

**Commit:** "Document handshake TLA+ spec and mark 1.2.4 complete"

______________________________________________________________________

## Verification

1. **TLC passes:** Run `make tlc-handshake` and confirm output contains
   "Model checking completed. No error has been found."

2. **Invariants documented:** Verify `MxdHandshake.tla` contains comments
   explaining each invariant.

3. **Integration test passes:** Run
   `cargo test -p mxd-verification -- --ignored` (requires Java + TLA+ tools).

4. **Quality gates pass:**
   - `make check-fmt`
   - `make lint`
   - `make test`
   - `make markdownlint`

## Design decisions

| Decision                         | Rationale                                                                                   |
| -------------------------------- | ------------------------------------------------------------------------------------------- |
| Discrete time ticks vs real time | Abstracts timing while preserving timeout semantics; simpler state space                    |
| MaxClients = 3                   | Balances state explosion vs concurrency coverage; can increase for deeper exploration       |
| Server-side model only           | Client actions are non-deterministic inputs; goal is verifying server invariants            |
| Separate crate for verification  | Keeps verification artefacts co-located; follows `docs/verification-strategy.md` convention |
| Docker wrapper for TLC           | Avoids local TLA+ Toolbox installation; reproducible across environments                    |
| CI on path-filtered PRs          | Runs TLC only when TLA+ specs or runner change; keeps CI fast for unrelated changes         |

## Files to modify

| File                                             | Change                    |
| ------------------------------------------------ | ------------------------- |
| `Cargo.toml` (root)                              | Add workspace member      |
| `crates/mxd-verification/Cargo.toml`             | New file                  |
| `crates/mxd-verification/src/lib.rs`             | New file                  |
| `crates/mxd-verification/tla/MxdHandshake.tla`   | New file                  |
| `crates/mxd-verification/tla/MxdHandshake.cfg`   | New file                  |
| `crates/mxd-verification/tests/tlc_handshake.rs` | New file                  |
| `scripts/run-tlc.sh`                             | New file (Docker wrapper) |
| `.github/workflows/tlc.yml`                      | New file (CI workflow)    |
| `Makefile`                                       | Add TLC targets           |
| `docs/verification-strategy.md`                  | Document handshake spec   |
| `docs/roadmap.md`                                | Mark 1.2.4 complete       |

## Open questions

None remaining.

## Progress log

- **[Planning]** Initial plan created based on exploration of codebase.
- **[Planning]** User confirmed: Docker wrapper for TLA+ tools, CI integration
  now (not deferred to 1.6.4).
