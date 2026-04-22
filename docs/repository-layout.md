# Repository layout

This document describes the organization of the mxd codebase and the purpose of
each major directory and file.

## Top-level structure

```text
mxd/
├── AGENTS.md              # Assistant coding guidelines
├── Cargo.lock             # Dependency lock file
├── Cargo.toml             # Workspace and package manifest
├── LICENSE                # Project licence
├── Makefile               # Build and development tasks
├── README.md              # Project overview and quick start
├── build.rs               # Build script for code generation
├── clippy.toml            # Clippy configuration
├── codecov.yml            # Code coverage configuration
├── diesel.toml            # Diesel Object–relational mapping (ORM) configuration
├── migrations/            # Database schema migrations
├── rust-toolchain.toml    # Rust toolchain specification
├── src/                   # Source code
├── tests/                 # Integration and behavioural tests
├── docs/                  # Documentation
├── crates/                # Additional crate workspaces
├── cli-defs/              # CLI definition utilities
├── fuzz/                  # Fuzzing harness
├── test-util/             # Test utilities and fixtures
└── validator/             # Protocol validation harness
```

## Source code organisation (`src/`)

The `src/` directory follows a hexagonal architecture pattern, separating
domain logic from infrastructure adapters.

```text
src/
├── lib.rs                 # Library entry point
├── main.rs                # Legacy server binary entry point
├── bin/                   # Additional binary entry points
│   ├── gen_corpus.rs      # Fuzzing corpus generator
│   └── mxd_wireframe_server.rs  # Wireframe server binary
├── db/                    # Database adapter (Diesel ORM)
├── domain/                # Core domain logic
├── server/                # Server runtime and CLI
│   ├── admin.rs           # Administrative subcommands
│   ├── cli.rs             # CLI argument parsing
│   ├── legacy.rs          # Legacy networking runtime
│   ├── wireframe.rs       # Wireframe bootstrap
│   └── mod.rs             # Server module entry
├── transaction/           # Hotline transaction handling
├── wireframe/             # Wireframe protocol adapter
└── wireframe_compat/      # Compatibility layer for Wireframe
```

## Documentation (`docs/`)

Documentation is organised by purpose and audience.

### Core documentation

- `design.md` — Comprehensive architecture and design decisions.
- `protocol.md` — Hotline protocol specification and implementation notes.
- `roadmap.md` — Implementation roadmap with phases and tasks.
- `users-guide.md` — End-user guide for running the server.
- `developers-guide.md` — Developer workflow and quality gates.

### Architecture and decisions

- `documentation-style-guide.md` — Conventions for authoring documentation.
- `adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md` —
  Hexagonal architecture adoption notes.
- `adr-*.md` — Architectural Decision Records (ADRs):
  - `adr-001-login-auth-reply-compatibility-layers.md`
  - `adr-002-compatibility-guardrails-and-augmentation.md`
  - `adr-003-login-authentication-and-reply-augmentation.md`

### Feature-specific documentation

- `chat-schema.md` — Chat system data model.
- `news-schema.md` — News system data model.
- `file-sharing-design.md` — File sharing architecture.
- `cte-extension-design.md` — Common Table Expression extension design.
- `internal-compatibility-matrix.md` — Client compatibility matrix.
- `verification-strategy.md` — Testing and verification approach.
- `fuzzing.md` — Fuzzing harness documentation.

### User guides

- `ortho-config-users-guide.md` — Configuration system guide.
- `pg-embed-setup-unpriv-users-guide.md` — PostgreSQL test helper guide.
- `pg-embed-setup-unpriv-v0-5-0-migration-guide.md` — Migration guide for
  PostgreSQL helper v0.5.0.
- `rstest-bdd-users-guide.md` — Behaviour-driven testing guide.
- `rstest-bdd-v0-5-0-migration-guide.md` — Migration guide for rstest-bdd
  v0.5.0.
- `rust-testing-with-rstest-fixtures.md` — Testing patterns with rstest.
- `rust-doctest-dry-guide.md` — Doctest patterns guide.
- `whitaker-users-guide.md` — Whitaker component guide.
- `wireframe-users-guide.md` — Wireframe library guide.
- `wireframe-v0-2-0-to-v0-3-0-migration-guide.md` — Wireframe v0.3.0 migration.

### Planning and process

- `migration-plan-moving-mxd-protocol-implementation-to-wireframe.md` —
  Protocol migration plan.
- `complexity-antipatterns-and-refactoring-strategies.md` — Refactoring
  guidance.
- `reliable-testing-in-rust-via-dependency-injection.md` — Testing strategies.
- `supporting-both-sqlite3-and-postgresql-in-diesel.md` — Dual-database
  support notes.
- `release-notes-qa-sign-off.md` — QA sign-off checklist.
- `mermaid-validation.md` — Mermaid diagram validation.
- `codescene-cli.md` — CodeScene CLI notes.

### Execution plans (`docs/execplans/`)

Detailed execution plans for roadmap tasks:

- `wireframe-v0-3-0-migration.md`
- `1-2-4-model-handshake-readiness.md`
- `1-3-4-kani-harnesses-for-transaction-framing-invariants.md`
- `1-4-2-route-transactions-through-wireframe.md`
- `1-4-3-introduce-a-shared-session-context.md`
- `1-4-4-outbound-transport-and-messaging-traits.md`
- `1-4-5-reply-builder.md`
- `1-4-6-model-routed-transactions-and-session-gating.md`
- `1-5-1-detect-clients-that-xor-encode-text-fields.md`
- `1-5-2-gate-protocol-quirks-on-the-handshake-sub-version.md`
- `1-5-3-internal-compatibility-matrix.md`
- `1-5-4-verify-xor-and-sub-version-compatibility.md`
- `1-5-6-split-login-authentication-strategies-from-reply-augmentation.md`
- `1-6-1-port-unit-and-integration-tests.md`
- `afl-circular-dependency-issue.md`
- `adopt-pg-embed-setup-v0-5-0.md`
- `adopt-rstest-bdd-v0-4-0.md`
- `adopt-rstest-bdd-v0-5-0.md`

## Tests (`tests/`)

```text
tests/
├── features/              # BDD feature files
│   ├── create_user_command/
│   ├── runtime_selection.feature
│   └── session_gating_verification.feature
├── transaction_streaming.rs
├── wireframe_handshake_metadata.rs
├── wireframe_transaction.rs
└── wireframe_xor_compat.rs
```

## Crates (`crates/`)

Additional workspace crates:

```text
crates/
└── mxd-verification/      # Formal verification and model checking
```

## Database migrations (`migrations/`)

Diesel database schema migrations for SQLite and PostgreSQL.

## Supporting directories

- `cli-defs/` — Shared CLI definition utilities.
- `fuzz/` — AFL++ fuzzing harness.
- `test-util/` — Shared test utilities and fixtures.
- `validator/` — Protocol validation using `hx` client and `expectrl`.
- `.cargo/` — Cargo configuration.
- `.config/` — Tool configurations (nextest, etc.).
- `.github/` — GitHub Actions workflows.
- `scripts/` — Development and utility scripts.
