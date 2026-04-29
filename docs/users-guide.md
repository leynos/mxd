# User guide

This guide explains how to run both server binaries and how to use their shared
administrative subcommands. The command-line interface (CLI) definitions live
in `mxd::server::cli`, while the active networking runtime is selected by the
`legacy-networking` Cargo feature.

## Launching the legacy server

- Build the sqlite variant with `make sqlite` or the postgres variant with
  `make postgres`. The `legacy-networking` feature is enabled by default, so
  the `mxd` binary is available.
- Start the daemon with `cargo run --bin mxd -- --bind 0.0.0.0:5500 --database
  mxd.db`.
- Override `MXD_`-prefixed environment variables or drop-in `.mxd.toml` files
  to persist defaults; the server merges file and env layers before applying
  CLI overrides via `ortho-config`.
- The listener prints `mxd listening on …` once the Tokio accept loop is
  running. Use `Ctrl-C` (and `SIGTERM` on Unix) for an orderly shutdown.

## Launching the Wireframe server

- Build the sqlite variant with `make APP=mxd-wireframe-server sqlite` or the
  postgres variant with `make APP=mxd-wireframe-server postgres`. The targets
  reuse the shared CLI module, so both binaries honour the same flags,
  environment overrides, and `.mxd.toml` defaults.
- Start the daemon with `cargo run --bin mxd-wireframe-server -- --bind
  0.0.0.0:6600 --database mxd.db`. The binary prints `mxd-wireframe-server
  listening on …` after the Wireframe listener binds.
- Administrative subcommands such as `create-user` remain available because
  the bootstrap calls `mxd::server::run_command` before starting the listener.
- The Wireframe listener now decodes the Hotline 12-byte handshake preamble,
  uses the upstream preamble hooks to send the 8-byte reply (0 on success, 1
  for invalid protocol, 2 for unsupported version, 3 for handshake timeout),
  drops idle sockets after the five-second handshake timeout before routing,
  and records the negotiated sub-protocol ID and sub-version in per-connection
  state. The metadata stays available for the lifetime of the connection, so
  compatibility shims can branch on client quirks, and it is cleared during
  teardown to avoid leaking between sessions. The transaction framing adapter
  keeps Hotline's native multi-fragment wire contract while configuring
  explicit inbound Wireframe budgets for one full logical request (20-byte
  header plus up to 1 MiB of payload). Fragmented requests above that cap are
  disconnected. If a client pauses a fragmented request for more than five
  seconds and then resumes it, the server closes the connection instead of
  routing the partial request. Valid fragmented requests that stay within the
  cap continue to route normally. Routing error replies preserve transaction
  IDs and types when a header is available, and routing failures are logged
  through the existing `tracing` infrastructure with transaction context.
- The wireframe adapter automatically detects clients that XOR-encode text
  fields (for example, SynHX with the `encode` toggle enabled). Once detected,
  inbound payloads are decoded and outbound replies are encoded to match the
  client, without any additional configuration.
- Login reply compatibility now depends on client metadata. SynHX is detected
  via handshake sub-version `2` and receives only the server version field
  (160). Hotline 1.8.5 and 1.9 clients are identified by the login request's
  version field (160) and receive the banner fields 161/162 in addition to the
  server version.
- Compatibility behaviour is unchanged by roadmap item 1.5.4; that milestone
  adds bounded Kani verification for XOR round-trips and login
  sub-version/version gating invariants.
- Internal release validation uses
  `docs/internal-compatibility-matrix.md` as the compatibility source of truth.
  Release-note QA sign-off must reference that matrix using the checklist in
  `docs/release-notes-qa-sign-off.md`.
- `WireframeRouter` is the sole public routing entrypoint. It embeds a
  `CompatibilityLayer` that applies XOR decoding, login-version recording, and
  banner-field augmentation on every routed transaction. Compatibility hooks
  cannot be accidentally bypassed by new routes. No user-visible behaviour
  change.
- Roadmap item 1.5.6 is complete: the guardrail routing entrypoint now wires
  `AuthStrategy` for login dispatch and `LoginReplyAugmenter` for login reply
  decoration. Default Hotline 1.8.5/1.9 and SynHX behaviour remains unchanged,
  so this is an internal architecture refactor rather than a user-visible
  protocol change.

## Selecting a runtime

- Keep the legacy loop enabled (default) to run the classic `mxd` daemon.
- Disable it with `--no-default-features --features "sqlite toml"` to obtain a
  Wireframe-only build; the `mxd` binary is skipped via `required-features` and
  `server::run()` invokes the Wireframe runtime.
- `make test-wireframe-only` exercises the Wireframe-first configuration and
  runs the behaviour scenarios that assert the feature gate.


## Listing nested news categories

News category list requests can target the root news hierarchy or a nested
bundle path. A root request lists every top-level bundle and category.
Supplying a bundle path lists only the bundles and categories directly below
that bundle, which lets clients browse a hierarchy one level at a time.

For protocol transaction 370 (`NewsCategoryNameList`), send the path parameter
as `/` for the root or as a slash-separated bundle path for a nested bundle.
For example:

```text
/
/Releases
/Releases/2026
```

If the hierarchy contains bundle `/Releases/2026` with categories
`Announcements` and `Maintenance`, a category-list request for `/Releases/2026`
returns those two names and omits sibling categories from `/Releases` or `/`. A
request for a missing path returns the same unsupported path error as other
invalid news lookups.

## Startup configuration reference

Both server binaries share the same startup configuration surface through
`AppConfig`.

- `--bind` / `MXD_BIND` set the listener bind address. Example:
  `cargo run --bin mxd -- --bind 0.0.0.0:5500`.
- `--database` / `MXD_DATABASE` set the database URL or sqlite path. Example:
  `MXD_DATABASE=postgres://localhost/mxd cargo run --bin mxd`.
- `--migration-timeout-secs` / `MXD_MIGRATION_TIMEOUT_SECS` map to the
  `migration_timeout_secs` field and set an optional migration timeout in
  seconds as an integer. Example:
  `cargo run --bin mxd -- --migration-timeout-secs 30` or
  `MXD_MIGRATION_TIMEOUT_SECS=30 cargo run --bin mxd-wireframe-server`.

When `--migration-timeout-secs` is unset, startup uses the built-in default
migration timeout. A value of `0` is normalized back to that default rather
than disabling the watchdog.

## File metadata baseline

Roadmap item 3.1.1 is an internal schema milestone rather than a new protocol
feature. Fresh databases now create the additive `file_nodes`, `permissions`,
`user_permissions`, `groups`, `user_groups`, and `resource_permissions` tables
used by the new file-sharing adapter, while the legacy `files` and `file_acl`
tables remain in place until roadmap item 3.1.2 backfills existing metadata.

There is no user-visible command change at this milestone:
`Get File Name List (200)` still behaves as before, but the backing lookup is
additive during the upgrade window. Fresh databases resolve visible top-level
entries from `file_nodes` and `resource_permissions` only, while upgraded or
mixed-state databases merge visible entries from `file_nodes`/
`resource_permissions` with legacy `files`/`file_acl` rows until roadmap item
3.1.2 backfills the new tables. This union is implemented in `src/db/files.rs`;
operators should treat mixed-state listings cautiously until backfill
completes. Folder traversal, alias operations, file comments, and
drop-box-specific transport behaviour remain scheduled for later roadmap items.
For the schema split and the planned backfill path, refer to `docs/design.md`
and `docs/file-sharing-design.md`.

## Creating users

The `create-user` subcommand now runs entirely inside the library so that it is
available to every binary. Supply both `--username` and `--password`; missing
values produce the same `missing username`/`missing password` errors that the
new `rstest-bdd` scenarios cover. Example:

```sh
cargo run --bin mxd -- create-user --username alice --password secret
```

The command runs pending migrations before inserting the user. Errors bubble up
unchanged, so the shell exit code remains reliable in automation scripts.

## Testing against PostgreSQL

Integration tests and developer machines can exercise the postgres backend by
installing the helper from crates.io and running it before `make test`:

```sh
cargo install --locked pg-embed-setup-unpriv
pg_embedded_setup_unpriv
```

The helper stages a PostgreSQL distribution with unprivileged ownership, so the
`postgresql_embedded` crate can start temporary clusters without root access.
After invoking the helper, run `make test` to execute sqlite, postgres, and
wireframe-only suites; the postgres jobs automatically reuse the staged
binaries. Both server binaries honour the same `MXD_DATABASE` and `--database`
values, allowing the helper to be re-run once and then switching between
`cargo run --bin mxd` and `cargo run --bin mxd-wireframe-server` without
additional setup. Refer to `docs/pg-embed-setup-unpriv-users-guide.md` for
cluster path customization and privilege troubleshooting details.

## Behaviour coverage

Unit tests for the CLI live next to `src/server/cli.rs` and rely on `rstest`
fixtures to validate configuration loading across env, dotfile, and CLI layers.
High-level behaviour tests are defined in `tests/features/create_user_command`
and `tests/features/runtime_selection.feature`, bound via `rstest-bdd`. They
cover successful account creation, missing-credential errors, and runtime
selection so the adapter strategy remains observable to end users. Wireframe
routing behaviour suites now run through the `mxd-wireframe-server` binary by
default via shared `test-util` harnesses, covering login compatibility,
authenticated-session continuity, file listing, and news routing flows in
`cargo test`. Verification-focused behaviour scenarios for the session gating
Stateright model live in `tests/features/session_gating_verification.feature`
and are bound by the `mxd-verification` test harness. Compatibility
verification also includes Kani harnesses in `src/wireframe/compat/kani.rs` and
`src/wireframe/compat_policy/kani.rs`, which prove bounded XOR and login-gating
invariants without changing runtime behaviour.
