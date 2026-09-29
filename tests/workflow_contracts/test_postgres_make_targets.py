"""The local PostgreSQL recipes carry the settings the embedded cluster needs.

`make test-postgres` and `make warm-postgres` are the local twins of the CI
steps that `test_embedded_postgres.py` holds, so a local run must not differ
from CI on the points that made CI fail: the nextest profile that serializes
the clusters, the pinned superuser password, throwaway directories for the
warm-up, and a warm-up that fails when it downloads nothing. The test recipe
is read from `make --dry-run`; the warm-up recipe is executed against a fake
`pg_embedded_setup_unpriv` placed where the Makefile's `PATH` looks first, so
the emptiness guard is driven rather than merely matched.
"""

from __future__ import annotations

import os
import subprocess
import typing as typ
from pathlib import Path

from ci_workflow_reader import REPO_ROOT

if typ.TYPE_CHECKING:
    import collections.abc as cabc

FAKE_NAME = "pg_embedded_setup_unpriv"


# Make runs `$(CARGO) nextest --version` while parsing. Under the fake home that
# would send rustup looking for a toolchain, so an inert command stands in: the
# recipes under test never invoke Cargo.
INERT_CARGO = "true"
AMBIENT = ("PG_PASSWORD", "PG_BINARY_CACHE_DIR", "PG_WARM_DIR")


def _make(
    *args: str, env: cabc.Mapping[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    """Run make in the repository root, without the caller's PostgreSQL settings.

    The variables are scrubbed because Make reads its `?=` defaults from the
    environment: a CI job that pins `PG_PASSWORD` would otherwise change what
    the default cases observe.
    """
    base = {k: v for k, v in os.environ.items() if k not in AMBIENT}
    return subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["make", f"CARGO={INERT_CARGO}", *args],  # noqa: S607 - make is on PATH
        cwd=REPO_ROOT,
        env={**base, **(env or {})},
        check=False,
        text=True,
        capture_output=True,
        timeout=60,
    )


def _dry_run(target: str, *overrides: str) -> str:
    """Return the recipe make would run for `target`."""
    result = _make("--dry-run", target, *overrides)
    assert result.returncode == 0, result.stdout + result.stderr
    return result.stdout


def test_test_postgres_selects_the_serializing_profile() -> None:
    """The recipe runs under the `postgres` nextest profile."""
    assert "NEXTEST_PROFILE=postgres" in _dry_run("test-postgres")


def test_test_postgres_pins_a_password_by_default() -> None:
    """A local run agrees on one superuser password across test processes."""
    assert 'PG_PASSWORD="mxd-embedded-test"' in _dry_run("test-postgres")


def test_test_postgres_password_can_be_overridden() -> None:
    """A caller's `PG_PASSWORD` wins over the default."""
    recipe = _dry_run("test-postgres", "PG_PASSWORD=caller-choice")
    assert 'PG_PASSWORD="caller-choice"' in recipe
    assert "mxd-embedded-test" not in recipe


def test_warm_postgres_uses_throwaway_directories() -> None:
    """The warm-up installs under `PG_WARM_DIR`, not the tests' cluster."""
    recipe = _dry_run("warm-postgres", "PG_WARM_DIR=/scratch/warm")
    assert 'PG_RUNTIME_DIR="/scratch/warm/install"' in recipe
    assert 'PG_DATA_DIR="/scratch/warm/data"' in recipe


class Host(typ.NamedTuple):
    """A fake home whose Cargo bin directory holds the stand-in downloader."""

    env: dict[str, str]
    cache: Path
    seen: Path


def _host(tmp_path: Path, *, populates_cache: bool) -> Host:
    """Build a host whose downloader fills the cache, or does not."""
    bin_dir = tmp_path / "home" / ".cargo" / "bin"
    bin_dir.mkdir(parents=True)
    cache = tmp_path / "cache"
    cache.mkdir()
    seen = tmp_path / "seen.txt"
    body = f'echo "$PG_RUNTIME_DIR $PG_DATA_DIR" > "{seen}"\n'
    if populates_cache:
        body += f'echo binary > "{cache}/postgres.tar"\n'
    fake = bin_dir / FAKE_NAME
    fake.write_text(f"#!/usr/bin/env sh\n{body}", encoding="utf-8")
    fake.chmod(0o755)
    env = {"HOME": str(tmp_path / "home"), "PG_BINARY_CACHE_DIR": str(cache)}
    return Host(env=env, cache=cache, seen=seen)


def test_warm_postgres_succeeds_when_the_cache_is_filled(tmp_path: Path) -> None:
    """A downloader that fills the cache passes, with the throwaway directories."""
    host = _host(tmp_path, populates_cache=True)
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=host.env)
    assert result.returncode == 0, result.stdout + result.stderr
    assert host.seen.read_text("utf-8").split() == [
        f"{tmp_path}/warm/install",
        f"{tmp_path}/warm/data",
    ]


def test_warm_postgres_fails_when_nothing_was_downloaded(tmp_path: Path) -> None:
    """An empty cache after the downloader ran fails the recipe."""
    host = _host(tmp_path, populates_cache=False)
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=host.env)
    assert result.returncode != 0, "an empty cache must fail the warm-up"


def test_warm_postgres_requires_a_cache_directory(tmp_path: Path) -> None:
    """Without `PG_BINARY_CACHE_DIR` the recipe names the missing variable."""
    host = _host(tmp_path, populates_cache=True)
    env = {"HOME": host.env["HOME"]}
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=env)
    assert result.returncode != 0, "a missing cache directory must fail"
    assert "PG_BINARY_CACHE_DIR" in result.stderr, result.stderr
