# Documentation contents

This file provides a guide to the mxd documentation, organized by purpose and
audience.

## Getting started

Start here for newcomers to the project.

- [`../README.md`](../README.md) — Project overview, quick start, and basic
  usage.
- [`users-guide.md`](users-guide.md) — Guide to running the server binaries
  and using administrative commands.
- [`developers-guide.md`](developers-guide.md) — Developer workflow, quality
  gates, and local setup.

## Understanding the architecture

Documents that explain the system's design and structure.

- [`design.md`](design.md) — Comprehensive architecture document covering the
  hexagonal (ports-and-adapters) design, runtime selection, and component
  boundaries.
- [`repository-layout.md`](repository-layout.md) — Organization of the codebase
  and the purpose of each directory.
- [`adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md`](adopting-hexagonal-architecture-in-the-mxd-wireframe-migration.md)
  — Notes on adopting hexagonal architecture during the Wireframe migration.

## Protocol and data models

Reference documentation for the Hotline protocol and data models.

- [`protocol.md`](protocol.md) — Hotline protocol specification, message
  formats, and transaction types.
- [`chat-schema.md`](chat-schema.md) — Chat system data model and schema.
- [`news-schema.md`](news-schema.md) — News system data model and schema.
- [`file-sharing-design.md`](file-sharing-design.md) — File sharing
  architecture and design.
- [`cte-extension-design.md`](cte-extension-design.md) — Common Table
  Expression extension for Diesel.

## Development guides

Detailed guides for specific development concerns.

### Testing

- [`rstest-bdd-users-guide.md`](rstest-bdd-users-guide.md) — Behaviour-driven
  development with rstest-bdd.
- [`rstest-bdd-v0-5-0-migration-guide.md`](rstest-bdd-v0-5-0-migration-guide.md)
  — Migrating to rstest-bdd v0.5.0.
- [`rust-testing-with-rstest-fixtures.md`](rust-testing-with-rstest-fixtures.md)
  — Testing patterns using rstest fixtures.
- [`rust-doctest-dry-guide.md`](rust-doctest-dry-guide.md) — Patterns for
  avoiding duplication in Rust doctests.
- [`reliable-testing-in-rust-via-dependency-injection.md`](reliable-testing-in-rust-via-dependency-injection.md)
  — Testing strategies using dependency injection.
- [`verification-strategy.md`](verification-strategy.md) — Overall testing and
  verification approach.
- [`fuzzing.md`](fuzzing.md) — Fuzzing harness setup and usage.

### Database

- [`supporting-both-sqlite3-and-postgresql-in-diesel.md`](supporting-both-sqlite3-and-postgresql-in-diesel.md)
  — Dual-database support implementation.
- [`pg-embed-setup-unpriv-users-guide.md`](pg-embed-setup-unpriv-users-guide.md)
  — PostgreSQL test helper guide.
- [`pg-embed-setup-unpriv-v0-5-0-migration-guide.md`](pg-embed-setup-unpriv-v0-5-0-migration-guide.md)
  — Migration guide for PostgreSQL helper v0.5.0.

### Configuration and utilities

- [`ortho-config-users-guide.md`](ortho-config-users-guide.md) — Configuration
  system using ortho-config.
- [`whitaker-users-guide.md`](whitaker-users-guide.md) — Whitaker component
  guide.
- [`wireframe-users-guide.md`](wireframe-users-guide.md) — Wireframe library
  usage guide.
- [`wireframe-v0-2-0-to-v0-3-0-migration-guide.md`](wireframe-v0-2-0-to-v0-3-0-migration-guide.md)
  — Migrating from Wireframe v0.2.0 to v0.3.0.

## Planning and roadmap

Documents for project planning and tracking progress.

- [`roadmap.md`](roadmap.md) — Implementation roadmap with phases, steps, and
  measurable tasks.
- [`migration-plan-moving-mxd-protocol-implementation-to-wireframe.md`](migration-plan-moving-mxd-protocol-implementation-to-wireframe.md)
  — Detailed plan for migrating protocol handling to Wireframe.
- [`execplans/`](execplans/) — Execution plans for individual roadmap tasks.

## Architectural decisions

Architectural Decision Records (ADRs) capture significant design decisions.

- [`adr-001-login-auth-reply-compatibility-layers.md`](adr-001-login-auth-reply-compatibility-layers.md)
  — ADR 001: Separate login authentication and reply augmentation (superseded).
- [`adr-002-compatibility-guardrails-and-augmentation.md`](adr-002-compatibility-guardrails-and-augmentation.md)
  — ADR 002: Compatibility guardrails and augmentation (superseded).
- [`adr-003-login-authentication-and-reply-augmentation.md`](adr-003-login-authentication-and-reply-augmentation.md)
  — ADR 003: Split login authentication and reply augmentation (accepted).

## Reference

Quick reference and miscellaneous documentation.

- [`documentation-style-guide.md`](documentation-style-guide.md) — Conventions
  for authoring documentation (spelling, formatting, ADR templates).
- [`internal-compatibility-matrix.md`](internal-compatibility-matrix.md) —
  Client compatibility matrix for release validation.
- [`release-notes-qa-sign-off.md`](release-notes-qa-sign-off.md) — QA sign-off
  checklist for releases.
- [`complexity-antipatterns-and-refactoring-strategies.md`](complexity-antipatterns-and-refactoring-strategies.md)
  — Guidance on recognizing and addressing code complexity.
- [`mermaid-validation.md`](mermaid-validation.md) — Validating Mermaid
  diagrams in documentation.
- [`codescene-cli.md`](codescene-cli.md) — CodeScene CLI notes.

## Document conventions

This documentation follows the conventions defined in
[`documentation-style-guide.md`](documentation-style-guide.md):

- British English (en-GB-oxendict) spelling.
- Sentence case for headings.
- 80-column wrap for paragraphs.
- 120-column wrap for code blocks.
- Oxford comma where it aids comprehension.
- Footnotes for references using `[^label]` syntax.
