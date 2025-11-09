# MXD user's guide

The Marrakesh Express daemon (`mxd`) ships two binaries: the CLI daemon that
hosts the legacy Tokio networking loop and the auxiliary tooling (for example,
`gen_corpus`). Regardless of which front-end you start, all runtime behaviour
flows through the shared `mxd` library so the protocol and domain logic stay
consistent.

## Starting the legacy server

1. Build the binary for the backend you need:
   - SQLite (default): `cargo run --bin mxd --features sqlite`
   - PostgreSQL: `cargo run --bin mxd --no-default-features --features postgres`
2. Provide the bind address and database location via CLI options, config file,
   or `MXD_` environment variables. Example:

   ```bash
   MXD_BIND=127.0.0.1:5500 MXD_DATABASE=/var/lib/mxd.db cargo run --bin mxd
   ```

3. The server prints `mxd listening on …` once the listener is ready. Press
   Ctrl-C (or send SIGTERM) to shut it down gracefully.

The CLI still exposes the `create-user` subcommand, which uses the configured
Argon2 parameters and runs Diesel migrations before inserting a record.

## Configuration validation

The legacy listener now lives in `mxd::transport::legacy`. The
`LegacyServerConfig` helper validates inputs before any sockets or database
pools are created:

- Bind addresses must parse into a `SocketAddr`. Typos (for example,
  `--bind 0.0.0.0:notaport`) fail fast with a clear error instead of bubbling
  up from Tokio later on.
- The database string must not be empty. Accidentally deleting `MXD_DATABASE`
  or passing `"   "` now produces `database url cannot be empty` immediately.

These checks fire for both SQLite paths and PostgreSQL URLs, so integration
smoke tests and operators receive the same early feedback.

## Troubleshooting

- **Handshake failures** – The server reports protocol issues in stdout/stderr
  while the new `rstest-bdd` scenarios exercise both happy and unhappy paths.
  If a change regresses handshake behaviour, `cargo test` will fail before you
  roll out a build.
- **Database migrations** – The listener still runs Diesel migrations on
  startup. Inspect the logs printed after `mxd listening on …` if a migration
  error is raised. Both backends continue to use the same code paths.
