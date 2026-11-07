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

import subprocess
import tomllib
import typing as typ

from ci_workflow_reader import REPO_ROOT

PROFILE = "rstest-bdd-ui"
PACKAGE = "rstest-bdd-ui-tests"

# Make runs `$(CARGO) nextest --version` while parsing. An inert command stands
# in, because a dry run must not need a working toolchain.
INERT_CARGO = "true"


def _nextest_config() -> dict[str, typ.Any]:
    """The parsed nextest configuration."""
    return tomllib.loads(
        (REPO_ROOT / ".config" / "nextest.toml").read_text(encoding="utf-8")
    )


def _dry_run(target: str) -> str:
    """Return the recipe make would run for `target`."""
    result = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["make", f"CARGO={INERT_CARGO}", "--dry-run", target],  # noqa: S607
        cwd=REPO_ROOT,
        check=True,
        text=True,
        capture_output=True,
        timeout=60,
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


def test_the_make_target_selects_the_ui_profile() -> None:
    """`make test-rstest-bdd-ui` runs under the profile, not just the override.

    The recipe is read from `make --dry-run` so that the assertion covers what
    Make actually expands, rather than the text of the file.
    """
    recipe = _dry_run("test-rstest-bdd-ui")
    assert f"NEXTEST_PROFILE={PROFILE}" in recipe, recipe
    assert PACKAGE in recipe, recipe
