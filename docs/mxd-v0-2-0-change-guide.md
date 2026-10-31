# mxd v0.2.0 change guide

This guide describes the configuration and command-line behaviour that lands in
the mxd v0.2.0 development line. No mxd release has been published yet, so
there is no released version to upgrade from. The guide exists so that anyone
running the server binaries from source knows what the current behaviour is and
can check a deployment against it.

## Scope

The changes are confined to the configuration adapter and the command-line
surface. The Hotline protocol, the database schema, and the administrative
commands behave as before. Only how configuration is discovered, layered, and
selected has changed.

## Configuration precedence

Values resolve in this order, from lowest to highest priority:

- built-in defaults;
- discovered configuration files;
- environment variables;
- values supplied explicitly on the command line.

Each field resolves on its own, so a file may set `database` while the
environment sets `argon2_m_cost`, and the merged result takes the
highest-priority value for each. An explicit command-line value wins even when
it equals the built-in default: passing `--bind` with the default address still
overrides a file or environment value.

The shared server configuration is available to both `mxd` and
`mxd-wireframe-server`, and each option has an environment-variable spelling:

| Option                     | Environment variable         |
| :------------------------- | :--------------------------- |
| `--bind`                   | `MXD_BIND`                   |
| `--database`               | `MXD_DATABASE`               |
| `--migration-timeout-secs` | `MXD_MIGRATION_TIMEOUT_SECS` |
| `--argon2-m-cost`          | `MXD_ARGON2_M_COST`          |
| `--argon2-t-cost`          | `MXD_ARGON2_T_COST`          |
| `--argon2-p-cost`          | `MXD_ARGON2_P_COST`          |

Table 1: Server configuration options and their environment variables.

When `--migration-timeout-secs` is unset, startup uses the built-in timeout. A
value of `0` is normalized back to that default rather than disabling the
migration watchdog.

## Selecting a configuration file

Automatic discovery may find nothing, which is not an error. To require one
specific file, set `MXD_CONFIG_PATH` or pass `--config-path PATH`. A selected
file becomes required:

- a selected file that does not exist fails startup instead of falling through
  to discovery;
- a selected file that cannot be read fails startup with the filesystem error;
- a selected file that cannot be parsed fails startup with the parse
  diagnostic.

In every failure case startup exits non-zero, writes the diagnostic to standard
error, and writes nothing to standard output.

## Global option placement

Global options belong before a subcommand. `create-user` takes its username and
password as positional arguments, so the invocation is:

```sh
mxd --database mxd.db create-user alice secret
```

A global option placed after the subcommand is rejected by the parser, as is a
`--` separator placed before the subcommand. An option value that happens to
spell a subcommand name is not treated as a subcommand boundary, so
`--database create-user` sets the database path rather than ending the global
options.

## Corrected documentation, not a parsing change

Earlier drafts of the users' guide showed `create-user` taking `--username` and
`--password` flags. The command has always accepted positional arguments; the
user guide example was wrong and has been corrected. No parsing behaviour
changed for that command, and no configuration key was renamed.

## Checking a setup

To confirm a deployment resolves configuration the way this guide describes:

1. Run the binary with `--help` and confirm the shared options are listed.
2. Set one option through a file and a different value through its
   environment variable, then start the server and confirm the environment
   value took effect.
3. Point `MXD_CONFIG_PATH` at a path that does not exist and confirm startup
   exits non-zero with a `load configuration` diagnostic.

The same behaviour is covered by the process-level tests in
`tests/cli_contracts.rs` and by the configuration loader tests in
`src/server/cli_tests.rs`.
