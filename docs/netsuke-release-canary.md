# Netsuke v0.1.0 release-admission canary

This branch is one of Netsuke's three v0.1.0 release-admission canaries.
Netsuke's release workflow checks this branch out at a pinned commit, builds
the exact Netsuke release candidate, and runs the `Netsukefile` gates below. A
failure blocks publication of the candidate only when it exposes a defect in
behaviour that v0.1.0 claims to support.

## Feature lanes

`MXD_BACKEND` selects exactly one feature lane: `postgres`, `sqlite`, or
`wireframe-only`. The `lint` and `test` actions are declared once per lane with
a manifest-time `when`, so Netsuke removes the other lanes before it builds the
graph. The lanes stay distinct: no gate collapses the mutually exclusive
features into `--all-features`, and a pass in one lane says nothing about
another.

| `MXD_BACKEND`    | Features                                                                     |
| ---------------- | ---------------------------------------------------------------------------- |
| `postgres`       | `--no-default-features --features 'postgres test-support legacy-networking'` |
| `sqlite`         | `--features 'sqlite test-support'`                                           |
| `wireframe-only` | `--no-default-features --features 'sqlite toml test-support'`                |

`env('MXD_BACKEND')` is required, so an unset selector fails to load. An
unrecognized value selects no lane, and `lint` and `test` then fail with the
accepted values rather than disappearing. Run one lane locally with, for
example:

```sh
MXD_BACKEND=sqlite netsuke build
```

The PostgreSQL lane expects `POSTGRES_TEST_URL`. In the release workflow the
canary job provides a PostgreSQL service and passes the connection string to
the gates without recording it in the canary's provenance.

## Retained boundaries

- The Makefile remains the canonical workflow for development, release,
  verification, validator, and database-server targets. Only the formatting,
  lint, and test slice moved to `Netsukefile`, and no action delegates back to
  `make`.
- Whitaker remains in MXD's own pinned CI lint job. The canary uses
  warning-denied Clippy for each lane, because its purpose is to validate
  Netsuke's explicit orchestration rather than to duplicate a Dylint suite.
- The explicit empty `targets: []` is retained because v0.1.0 requires the
  top-level key for an action-only manifest.
- The test actions keep the Makefile's `cargo nextest` commands and validator
  exclusion unchanged.
