# v0.6.0 migration guide (from v0.5.x)

This guide covers mxd's move from `pg-embed-setup-unpriv` 0.5.2 to 0.6.3. It is
written for maintainers of mxd's test harness rather than for users of the
crate, and it records what actually changed in this repository together with
the evidence behind each decision.

## Scope and outcome

The migration keeps the upstream handle/guard split,
`test_support::shared_cluster_handle()`, template helpers, and the `async-api`
signatures mxd uses compatible. The harness adds bounded slot-race retries and
a cached fallback for template startup. The `diesel-support` feature is not
needed, because the harness returns connection URLs as strings and mxd opens
its own diesel connections.

The release changes the isolation model: every cluster gets its own data
directory, so the postgres leg no longer has to run one test at a time.

## Quick upgrade checklist

- Set `pg-embedded-setup-unpriv` to `0.6.3` in `test-util/Cargo.toml` with the
  `async-api` feature, and keep one version in `Cargo.lock` (`cargo tree -d`).
- Remove any root-crate dev dependency on the crate that imports nothing.
- Confirm a SQLite-only build does not pull the harness in.
- Pin `PG_VERSION_REQ` to `=18.6.0`, as described below.
- Remove the single-slot nextest test group and its `all()` override.
- Remove the `#[serial_test::file_serial(...)]` attributes.
- Run `make test-postgres` and compare its wall time with the serial baseline.

## Behavioural changes that matter here

### Each cluster gets its own data directory

Under 0.5.x every cluster in a run shared one data directory, and the guard
that dropped a cluster deleted it, so two tests starting clusters at once could
collide. The serial nextest group and the `file_serial` attributes both existed
to prevent that collision.

0.6 places each cluster's data on the install root, under `<root>/data`, so the
shared-data collision is gone. Both serialization mechanisms are removed and
the leg runs at the default parallelism. `tests/postgres_harness_isolation.rs`
keeps two `PostgresTestDb` fixtures alive at once, asserts their database names
differ, and confirms a table created through one is invisible to the other.

### Slot contention and shared template startup

Separate data directories do not prevent the 0.6.3 slot-claim race, which can
return `slot lock ... is already held`. `retry_slot_contention` makes up to
eight start attempts, with backoff increasing in 25 ms steps. The
`is_slot_contention` check searches the error chain for both `slot lock` and
`is already held`.

Template startup first uses the upstream shared-cluster initializer. If that
initializer reports slot contention, `shared.rs` creates a separate shared
cluster through the retry path, caches the fallback result, and registers its
exit-time shutdown hook. Other upstream initialization errors remain failures.

### The password is stored with the cluster and reused

0.5.2 generated a superuser password per process. A test that left state in the
shared template cluster then made the next process fail to authenticate,
because that process held a different password. 0.6 stores the password with
the cluster and reuses it, with `stored_cluster_password` and
`reuse_existing_password` exposing the behaviour.

mxd still sets a fixed `PG_PASSWORD`, because an explicit password overrides
the stored one, and every process in a run must agree on it. It remains a
throwaway value: the clusters listen on localhost and live only as long as the
run.

### Install root and the binary cache

0.6 installs under a root containing `install/` and `data/`, with a lock on the
install tree. `PG_EMBED_ROOT` names that root; `PG_RUNTIME_DIR` and
`PG_DATA_DIR` still name the two directories individually. `PG_MAX_CONNECTIONS`
caps connections per cluster (default 20, minimum 4).

`make warm-postgres` keeps its shape: it points `PG_RUNTIME_DIR` and
`PG_DATA_DIR` at throwaway directories under `PG_WARM_DIR`, so the warm-up
leaves nothing behind in the test cluster's location, and it checks that
`PG_BINARY_CACHE_DIR` is no longer empty. It now passes the same
`PG_VERSION_REQ` as `make test-postgres`, because the binary cache is keyed by
version and a warm-up that resolved a different release would leave the tests
to download their own.

## The bundle pin, and why the version alone is not enough

Selecting 0.6.3 does not select a working PostgreSQL bundle, and this is the
part of the migration that is specific to the moment of writing.

The October theseus rebuilds (18.6.1 and later) are built `--with-icu` without
`--with-system-tzdata`. They therefore ship their own `share/timezone` and
express the time-zone aliases as tar hard links: 18.6.1 contains 257 of them,
including `share/timezone/UTC` pointing at `UCT`. The `postgresql_archive`
release that 0.6.3 depends on, 0.20.x, extracts directories, files and symlinks
but skips hard links, so `share/timezone/UTC` never exists. PostgreSQL rejects
`SET TIME ZONE 'UTC'`; diesel's `PgConnection::establish` sends exactly that
during connect, so every database test fails at its first connection with
`invalid value for parameter "TimeZone": "UTC"` rather than in an assertion.

Hard-link extraction arrives in `postgresql_archive` 0.22.0, which requires
Rust 1.94.0 and which no published `pg-embed-setup-unpriv` release can select.
The fix is therefore unreachable from every release on crates.io at the time of
writing, irrespective of the minimum supported Rust version.

The August 18.6.0 asset was created and last updated on 2026-08-15, is 12 MB,
was built `--with-system-tzdata=/usr/share/zoneinfo`, carries no
`share/timezone`, and accepts `UTC`.

`PG_VERSION_REQ` is parsed verbatim into a semver requirement, and release
selection takes the highest match. A bare version is not an exact pin, so it
can select 18.6.1 again. Alternate bundle values must use an exact requirement,
including the leading `=`. The pin is:

```make
PG_VERSION_REQ ?= =18.6.0
```

The Makefile passes it to both PostgreSQL targets, and each CI job that runs
the PostgreSQL tests sets it next to `PG_PASSWORD`.
`cluster_accepts_the_utc_time_zone` in `tests/postgres_harness_isolation.rs`
fails if a bundle with incomplete time-zone data is selected. Remove the pin
once a `pg-embed-setup-unpriv` release can select `postgresql_archive` 0.22.0
or newer.

## API changes at a glance

| 0.5.x                                   | 0.6.x                                      | mxd impact                                    |
| --------------------------------------- | ------------------------------------------ | --------------------------------------------- |
| One shared data directory per run       | One data directory per cluster             | Removed the serialization                     |
| Password generated per process          | Password stored with the cluster           | Fixed `PG_PASSWORD` still set                 |
| `PG_RUNTIME_DIR`, `PG_DATA_DIR`         | Plus `PG_EMBED_ROOT`, `PG_MAX_CONNECTIONS` | None; existing variables still work           |
| `bootstrap`/`test_support` module paths | Unchanged for what mxd uses                | None                                          |
| `async-api`, `diesel-support` features  | Same names                                 | `async-api` kept, `diesel-support` not needed |

## Suggested migration validation

1. Bundle and time zone

   - Run `make warm-postgres` with `PG_BINARY_CACHE_DIR` set, and confirm the
     cache is populated for `18.6.0`.
   - Run the PostgreSQL-only test with the same feature set and profile as
     `make test-postgres`, and confirm both `UTC` and `Etc/UTC` resolve:

     ```sh
     PG_VERSION_REQ='=18.6.0' NEXTEST_PROFILE=postgres \
       cargo nextest run --no-default-features \
       --features "postgres test-support legacy-networking" \
       -E 'test(cluster_accepts_the_utc_time_zone)'
     ```

2. Isolation

   - Run the isolation test with the same bundle, feature set, and profile as
     `make test-postgres`:

     ```sh
     PG_VERSION_REQ='=18.6.0' NEXTEST_PROFILE=postgres \
       cargo nextest run --no-default-features \
       --features "postgres test-support legacy-networking" \
       -E 'test(concurrent_test_databases_are_isolated)'
     ```

3. Parallel leg

   - Run `make test-postgres` with no `pg-embed` group and record the wall
     time, the five slowest tests and the timeout count.
   - Confirm no cluster data directories or server processes are left behind.

4. Workflows

   - Run `make test-workflow-contracts`; the embedded-PostgreSQL and
     make-target contracts cover the bundle pin, the removed group and the
     helper version matching `Cargo.lock`.

## Notes for CI maintainers

- Do not raise the profile's 300 second slow timeout or 60 minute global
  timeout to absorb a slower run. A slower parallel run is a finding to report,
  not a budget to bump.
- There is no binary-cache restore across CI runs. Every job warms the cache
  from scratch, which keeps the warm-up's cost visible and its failure
  attributable to that run.
- The `binstall` version in both workflows must equal the version `Cargo.lock`
  holds; the contract fails otherwise, because a mismatch fills a binary cache
  for a version the tests do not use.
