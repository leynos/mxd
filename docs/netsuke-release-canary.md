# Netsuke v0.1.0 release-admission canary

This branch runs formatting plus distinct PostgreSQL, SQLite, and
wireframe-only lint and test actions through the pinned Netsuke candidate.
Feature lanes remain separate actions because combining them would hide
mutually exclusive runtime contracts and make a pass non-diagnostic.

The test actions use the same `cargo nextest` commands and validator exclusion
as the canonical Makefile. The PostgreSQL CI matrix entry provisions the
repository's normal PostgreSQL service and exports `POSTGRES_TEST_URL`; local
embedded-PostgreSQL behaviour is not substituted for the CI contract.

The explicit empty `targets: []` is retained because v0.1.0 still requires the
top-level key for an action-only canary.

The Makefile remains for the wider developer, release, verification, and
database-server workflows. This canary's selected commands are defined directly
in `Netsukefile`; it does not delegate a Netsuke action back to `make`.

Whitaker remains in MXD's independently pinned CI lint job. The release canary
uses warning-denied Clippy for each feature lane because its purpose is to
validate Netsuke's explicit orchestration, not to duplicate a
toolchain-specific Dylint suite whose current helper findings are unrelated to
Netsuke.
