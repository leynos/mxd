"""The rstest-bdd UI gate keeps the time budget its trybuild fixtures need.

`tests/rstest-bdd-ui` shells out to Cargo once per trybuild fixture, and
trybuild runs `cargo clean --package` before each of those builds, so the
fixture package is rebuilt rather than reused. Those builds also re-enter the
shared compile-admission queue behind whatever else the host is building. A
300-second ceiling was already too tight there: the gate timed out on `main`
itself, with only two fixtures.

`global-timeout` is not one of the settings a per-package override can replace,
so the run-wide headroom lives in the `rstest-bdd-ui` profile, which
`make test-rstest-bdd-ui` selects through `NEXTEST_PROFILE`. These contracts
pin all three parts, because dropping the profile selection would leave the
override's raised `slow-timeout` underneath the default 10-minute run ceiling.
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import tomllib
import typing as typ

from ci_workflow_reader import REPO_ROOT

PROFILE = "rstest-bdd-ui"
PACKAGE = "rstest-bdd-ui-tests"

# Make chooses its test runner from `$(shell $(CARGO) nextest --version)`.
# A stub named `cargo` that answers that one probe therefore makes a dry run
# expand exactly the `nextest run` recipe CI uses, without needing a toolchain.
# Passing `CARGO=true` instead would resolve to `cargo test`, so the assertion
# would pass while covering the wrong command.
CARGO_STUB = """#!/bin/sh
if [ "$1" = "nextest" ] && [ "$2" = "--version" ]; then
    echo "cargo-nextest stub"
fi
exit 0
"""


def _nextest_config() -> dict[str, typ.Any]:
    """The parsed nextest configuration."""
    return tomllib.loads(
        (REPO_ROOT / ".config" / "nextest.toml").read_text(encoding="utf-8")
    )


def _dry_run(target: str, tmp_path: pathlib.Path) -> str:
    """Return the recipe make would run for `target`."""
    cargo = tmp_path / "cargo"
    cargo.write_text(CARGO_STUB, encoding="utf-8")
    cargo.chmod(0o755)
    result = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["make", "--dry-run", target],  # noqa: S607
        cwd=REPO_ROOT,
        check=True,
        text=True,
        capture_output=True,
        timeout=60,
        env={**os.environ, "PATH": f"{tmp_path}{os.pathsep}{os.environ['PATH']}"},
    )
    return result.stdout


def test_the_ui_profile_raises_the_run_wide_timeout() -> None:
    """The profile exists, and raises the ceiling the override cannot reach.

    The per-package override only replaces `slow-timeout`. Without a profile
    carrying a larger `global-timeout`, a cold trybuild build can still be cut
    off by the default 10-minute run ceiling.
    """
    config = _nextest_config()
    profile = config["profile"][PROFILE]
    assert profile["global-timeout"] == "30m", profile
    assert profile["slow-timeout"]["period"] == "900s", profile["slow-timeout"]


def test_the_package_override_matches_the_profile() -> None:
    """The default-profile override keeps the same generous slow timeout.

    A run that reaches the package without selecting the profile should still
    treat a slow trybuild build as slow rather than failed at 300 seconds.
    """
    config = _nextest_config()
    overrides = config["profile"]["default"].get("overrides") or []
    matching = [o for o in overrides if o.get("filter") == f"package({PACKAGE})"]
    assert matching, f"no override for {PACKAGE}: {overrides}"
    for override in matching:
        assert override["slow-timeout"]["period"] == "900s", override


def test_the_make_target_selects_the_ui_profile(tmp_path: pathlib.Path) -> None:
    """`make test-rstest-bdd-ui` runs nextest under the profile.

    The recipe is read from `make --dry-run` so that the assertion covers what
    Make actually expands, rather than the text of the file. Asserting the
    whole invocation catches a profile assignment that is emitted but never
    reaches `nextest`, which an independent substring check would miss.
    """
    recipe = _dry_run("test-rstest-bdd-ui", tmp_path)
    assert f"NEXTEST_PROFILE={PROFILE}" in recipe, recipe
    assert f"cargo nextest run -p {PACKAGE}" in recipe, recipe


def test_the_profile_and_override_share_one_budget() -> None:
    """Both routes into the package allow the same slow timeout.

    Whichever way a run reaches `rstest-bdd-ui-tests`, the ceiling it is held
    to comes from the same number, so the two cannot drift apart.
    """
    config = _nextest_config()
    profile_period = config["profile"][PROFILE]["slow-timeout"]["period"]
    override_periods = {
        override["slow-timeout"]["period"]
        for override in config["profile"]["default"].get("overrides") or []
        if override.get("filter") == f"package({PACKAGE})"
    }
    assert override_periods == {profile_period}, override_periods
