"""The local PostgreSQL recipes carry the settings the embedded cluster needs.

`make test-postgres` and `make warm-postgres` are the local twins of the CI
steps that `test_embedded_postgres.py` holds, so a local run must not differ
from CI on the points that made CI fail: the PostgreSQL nextest profile, the
pinned superuser password, no PostgreSQL bundle pin (the harness chooses),
throwaway directories for the warm-up, and a warm-up that fails when it
downloads nothing. The profile runs at default parallelism; the former
single-slot group is removed. The test recipe is read from `make --dry-run`;
the warm-up recipe is executed against a fake `pg_embedded_setup_unpriv`
placed where the Makefile's `PATH` looks first, so the emptiness guard is
driven rather than merely matched.
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
AMBIENT = ("PG_PASSWORD", "PG_BINARY_CACHE_DIR", "PG_WARM_DIR", "PG_VERSION_REQ")


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


def _recipe_environment(*overrides: str) -> dict[str, str]:
    """Return the environment a Makefile recipe receives.

    A probe target is appended with `--eval`, so exports declared in the Makefile
    (which never appear in `--dry-run` text) show up in what the recipe sees.
    """
    result = _make(
        "--eval=envprobe:\n\t@env",
        "envprobe",
        *overrides,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    return dict(
        line.split("=", 1) for line in result.stdout.splitlines() if "=" in line
    )


def test_makefile_exports_no_bundle_requirement() -> None:
    """With the caller's variable absent, no recipe environment carries a pin."""
    assert "PG_VERSION_REQ" not in _recipe_environment()


def test_a_caller_supplied_bundle_requirement_is_not_rewritten() -> None:
    """A caller's exact requirement reaches recipes as given, not replaced."""
    assert (
        _recipe_environment("PG_VERSION_REQ==17.11.0")["PG_VERSION_REQ"] == "=17.11.0"
    )


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


def test_postgres_recipes_do_not_pin_the_bundle() -> None:
    """Neither recipe sets `PG_VERSION_REQ`: the harness selects the bundle."""
    assert "PG_VERSION_REQ" not in _dry_run("test-postgres")
    assert "PG_VERSION_REQ" not in _dry_run(
        "warm-postgres", "PG_WARM_DIR=/scratch/warm"
    )


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


class CacheLayout(typ.NamedTuple):
    """What the fake downloader leaves in the cache and the install tree.

    `populates_cache` controls whether anything is downloaded at all.
    `dangling_utc` leaves a broken `UTC` symlink behind in the cache, the shape
    a half-run warm-up creates. `installs_runtime_root` controls whether the
    downloader creates the install tree the recipe scans second; without it the
    scan meets a path that does not exist. `symlinked_cache_root` points
    `PG_BINARY_CACHE_DIR` at a symbolic link to the cache, as a deployment that
    keeps the cache on another volume would.
    """

    populates_cache: bool = True
    dangling_utc: bool = False
    installs_runtime_root: bool = True
    symlinked_cache_root: bool = False


def _host(tmp_path: Path, layout: CacheLayout = CacheLayout()) -> Host:
    """Build a host whose downloader leaves the cache in `layout`'s shape."""
    bin_dir = tmp_path / "home" / ".cargo" / "bin"
    bin_dir.mkdir(parents=True)
    cache = tmp_path / "cache"
    cache.mkdir()
    scan_root = cache
    if layout.symlinked_cache_root:
        scan_root = tmp_path / "cache-link"
        scan_root.symlink_to(cache, target_is_directory=True)
    seen = tmp_path / "seen.txt"
    body = f'echo "$PG_RUNTIME_DIR $PG_DATA_DIR" > "{seen}"\n'
    if layout.populates_cache:
        body += f'echo binary > "{cache}/postgres.tar"\n'
        timezone_dir = cache / "18.6.1" / "share" / "timezone"
        body += f'mkdir -p "{timezone_dir}"\n'
        body += f'touch "{timezone_dir / "UCT"}"\n'
        if layout.dangling_utc:
            body += f'ln -s nowhere "{timezone_dir / "UTC"}"\n'
        if layout.installs_runtime_root:
            body += 'mkdir -p "$PG_RUNTIME_DIR/18.6.1/share/timezone"\n'
            body += 'touch "$PG_RUNTIME_DIR/18.6.1/share/timezone/UCT"\n'
    fake = bin_dir / FAKE_NAME
    fake.write_text(f"#!/usr/bin/env sh\n{body}", encoding="utf-8")
    fake.chmod(0o755)
    env = {"HOME": str(tmp_path / "home"), "PG_BINARY_CACHE_DIR": str(scan_root)}
    return Host(env=env, cache=cache, seen=seen)


def test_warm_postgres_succeeds_when_the_cache_is_filled(tmp_path: Path) -> None:
    """Warm-up fills the cache, fixes its UTC alias and uses throwaway directories."""
    host = _host(tmp_path)
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=host.env)
    assert result.returncode == 0, result.stdout + result.stderr
    assert host.seen.read_text("utf-8").split() == [
        f"{tmp_path}/warm/install",
        f"{tmp_path}/warm/data",
    ]
    runtime_install = Path(host.seen.read_text("utf-8").split()[0])
    utc_aliases = [
        host.cache / "18.6.1" / "share" / "timezone" / "UTC",
        runtime_install / "18.6.1" / "share" / "timezone" / "UTC",
    ]
    for utc_alias in utc_aliases:
        assert utc_alias.is_symlink(), f"expected {utc_alias} to be a symlink"
        assert os.readlink(utc_alias) == "UCT", f"expected {utc_alias} to point to UCT"


def test_warm_postgres_replaces_a_dangling_utc_alias(tmp_path: Path) -> None:
    """A broken alias from an earlier run is replaced, not left dangling.

    `ln -s` fails when the link already exists and `-e` is false for a link
    whose target is gone, so a recipe that only guards on existence would keep
    the broken link while still reporting success.
    """
    host = _host(tmp_path, CacheLayout(dangling_utc=True))
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=host.env)
    assert result.returncode == 0, result.stdout + result.stderr
    cache_alias = host.cache / "18.6.1" / "share" / "timezone" / "UTC"
    assert cache_alias.is_symlink(), f"expected {cache_alias} to be a symlink"
    assert os.readlink(cache_alias) == "UCT", f"expected {cache_alias} to be repaired"


def test_warm_postgres_follows_a_symlinked_cache_root(tmp_path: Path) -> None:
    """A cache reached through a symbolic link is still scanned.

    `find` does not descend into a symbolic link named on its command line
    unless `-H` is given, so a cache kept behind a link would keep its missing
    `UTC` alias while the recipe still reported the install tree's success.
    """
    host = _host(tmp_path, CacheLayout(symlinked_cache_root=True))
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=host.env)
    assert result.returncode == 0, result.stdout + result.stderr
    cache_alias = host.cache / "18.6.1" / "share" / "timezone" / "UTC"
    assert cache_alias.is_symlink(), f"expected {cache_alias} to be repaired"
    assert os.readlink(cache_alias) == "UCT", f"expected {cache_alias} to point to UCT"


def test_warm_postgres_fails_when_an_alias_root_is_missing(tmp_path: Path) -> None:
    """A scan root that does not exist fails the recipe, rather than being skipped.

    The loop visits the cache and the install tree. A downloader that leaves the
    install tree absent must not turn the second scan's failure into success.
    """
    host = _host(tmp_path, CacheLayout(installs_runtime_root=False))
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=host.env)
    assert result.returncode != 0, "a missing install root must fail the warm-up"


def test_warm_postgres_fails_when_nothing_was_downloaded(tmp_path: Path) -> None:
    """An empty cache after the downloader ran fails the recipe."""
    host = _host(tmp_path, CacheLayout(populates_cache=False))
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=host.env)
    assert result.returncode != 0, "an empty cache must fail the warm-up"


def test_warm_postgres_requires_a_cache_directory(tmp_path: Path) -> None:
    """Without `PG_BINARY_CACHE_DIR` the recipe names the missing variable."""
    host = _host(tmp_path)
    env = {"HOME": host.env["HOME"]}
    result = _make("warm-postgres", f"PG_WARM_DIR={tmp_path}/warm", env=env)
    assert result.returncode != 0, "a missing cache directory must fail"
    assert "PG_BINARY_CACHE_DIR" in result.stderr, result.stderr
