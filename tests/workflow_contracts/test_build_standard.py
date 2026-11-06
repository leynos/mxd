"""Contracts for the Rust build standard.

Development builds use the parallel `rustc` frontend and, on Linux, the `mold`
linker. Cargo selects one `rustflags` source rather than merging them, so
`.cargo/config.toml` repeats the frontend flag in `[build]` and in the Linux
target table, and keeps the linker flag in the Linux table alone. The
`build-test` job exports the same flags through `setup-rust`. The files are
parsed, so a reformat or a moved comment cannot break these checks. Cranelift
is not the development backend, so no backend key may appear.
"""

from __future__ import annotations

import tomllib
import typing as typ

import pytest
from ci_workflow_reader import REPO_ROOT, repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

THREADS: typ.Final = "-Zthreads=8"
MOLD: typ.Final = "-Clink-arg=-fuse-ld=mold"


@pytest.fixture(scope="module")
def config() -> dict[str, typ.Any]:
    """The parsed Cargo configuration."""
    text = (REPO_ROOT / ".cargo" / "config.toml").read_text(encoding="utf-8")
    return tomllib.loads(text)


def _target_tables(config: cabc.Mapping[str, typ.Any]) -> dict[str, dict[str, typ.Any]]:
    """Every `[target.*]` table that declares `rustflags`, by key."""
    targets = config.get("target", {})
    return {key: table for key, table in targets.items() if "rustflags" in table}


def test_the_build_table_carries_the_parallel_frontend(
    config: cabc.Mapping[str, typ.Any],
) -> None:
    """`[build] rustflags` is the source non-Linux hosts fall through to."""
    assert THREADS in config["build"]["rustflags"]
    assert MOLD not in config["build"]["rustflags"]


def test_a_linux_table_repeats_the_frontend_beside_the_linker(
    config: cabc.Mapping[str, typ.Any],
) -> None:
    """The Linux table replaces `[build]`, so it restates every flag."""
    linux = [
        table["rustflags"]
        for key, table in _target_tables(config).items()
        if "linux" in key
    ]
    assert linux, "no target table applies on Linux"
    for flags in linux:
        assert THREADS in flags
        assert MOLD in flags


def test_no_other_target_table_names_the_linker(
    config: cabc.Mapping[str, typ.Any],
) -> None:
    """`mold` ships for Linux alone."""
    others = [
        key
        for key, table in _target_tables(config).items()
        if "linux" not in key and MOLD in table["rustflags"]
    ]
    assert not others, f"these target tables name the linker: {others}"


def test_cranelift_is_not_selected(config: cabc.Mapping[str, typ.Any]) -> None:
    """The suite is not measured under it, so neither backend key is set."""
    profile = config.get("profile", {}).get("dev", {})
    assert "codegen-backend" not in profile
    assert "codegen-backend" not in config.get("unstable", {})


def test_the_build_test_job_exports_the_standard_flags() -> None:
    """The action's `rustflags` input restates the flags, and mold is installed."""
    job = repository_documents()["ci.yml"]["jobs"]["build-test"]
    setup = next(
        step
        for step in job["steps"]
        if str(step.get("uses", "")).startswith(
            "leynos/shared-actions/.github/actions/setup-rust@"
        )
    )
    inputs = setup["with"]
    assert THREADS in inputs["rustflags"]
    assert MOLD in inputs["rustflags"]
    assert str(inputs["install-mold"]).lower() == "true"
