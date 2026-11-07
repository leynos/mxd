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

A raised `slow-timeout` is only half of a time budget: the period says when a
test counts as slow, and `terminate-after` says whether that kills it or merely
reports it. Both are asserted on every route into the package, so the profile
cannot be reduced to a longer warning.
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import tomllib
import typing as typ

import yaml

from ci_workflow_reader import REPO_ROOT

TARGET = "test-rstest-bdd-ui"
PROFILE = "rstest-bdd-ui"
PACKAGE = "rstest-bdd-ui-tests"

# Make chooses its test runner from `$(shell $(CARGO) nextest --version)` and
# then runs that command for real, so a stub standing in for Cargo has to answer
# the probe and absorb the run. It records the profile it inherited and the
# arguments it was handed into `$RECORDED`, which is what the contracts assert
# on: the recipe's text says where `NEXTEST_PROFILE=` appears, but only the
# process environment says whether nextest actually received it. Substituting
# `CARGO=true` instead would resolve to `cargo test` and cover the wrong command
# entirely.
#
# The stub is passed as `CARGO=<path>` rather than put on `PATH`. The Makefile
# prepends `$(HOME)/.cargo/bin` to `PATH` on its third line, so a stub directory
# on the caller's `PATH` is shadowed by the real Cargo no matter where the
# caller puts it.
CARGO_STUB = """#!/bin/sh
if [ "$1" = "nextest" ] && [ "$2" = "--version" ]; then
    echo "cargo-nextest stub"
    exit 0
fi
{
    printf 'profile=%s\\n' "${NEXTEST_PROFILE-}"
    for argument in "$@"; do
        printf 'argument=%s\\n' "$argument"
    done
} > "$RECORDED"
exit 0
"""

# nextest writes its durations in whole units, so a plain suffix table is the
# whole grammar the contracts need to compare one timeout against another.
DURATION_UNITS = {"s": 1, "m": 60}


class Invocation(typ.NamedTuple):
    """What the Cargo stub was asked to do by the Make target."""

    profile: str
    arguments: tuple[str, ...]


def _nextest_config() -> dict[str, typ.Any]:
    """The parsed nextest configuration."""
    return tomllib.loads(
        (REPO_ROOT / ".config" / "nextest.toml").read_text(encoding="utf-8")
    )


def _package_overrides(config: dict[str, typ.Any]) -> list[dict[str, typ.Any]]:
    """Every default-profile override that targets the UI package."""
    overrides = config["profile"]["default"].get("overrides") or []
    return [
        override
        for override in overrides
        if override.get("filter") == f"package({PACKAGE})"
    ]


def _parse_recorded(text: str) -> Invocation:
    """Parse the stub's record into the profile and the arguments given."""
    profile = ""
    arguments: list[str] = []
    for line in text.splitlines():
        key, _, value = line.partition("=")
        if key == "profile":
            profile = value
        elif key == "argument":
            arguments.append(value)
    return Invocation(profile, tuple(arguments))


def _invoke_target(tmp_path: pathlib.Path) -> Invocation:
    """Run the real Make target and report what reached the Cargo stub.

    The target is executed rather than dry-run: a dry run prints the recipe
    Make would use, but only an execution puts `NEXTEST_PROFILE` into the
    environment of the process the recipe starts. The stub absorbs the run, so
    no build takes place and the elapsed cost is Make's own startup.
    """
    cargo = tmp_path / "cargo"
    cargo.write_text(CARGO_STUB, encoding="utf-8")
    cargo.chmod(0o755)
    recorded = tmp_path / "recorded"
    # The recipe's own assignment is what this contract is about, so the
    # caller's `NEXTEST_PROFILE` is removed: a CI job that pins one would
    # otherwise be indistinguishable from a recipe that sets it, and the
    # contract would pass with the assignment deleted from the Makefile.
    inherited = {k: v for k, v in os.environ.items() if k != "NEXTEST_PROFILE"}
    subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["make", f"CARGO={cargo}", TARGET],  # noqa: S607
        cwd=REPO_ROOT,
        check=True,
        text=True,
        capture_output=True,
        timeout=120,
        env={**inherited, "RECORDED": str(recorded)},
    )
    return _parse_recorded(recorded.read_text(encoding="utf-8"))


def _option_value(arguments: tuple[str, ...], option: str) -> str | None:
    """Return the value that follows `option`, or `None` when it is absent."""
    try:
        index = arguments.index(option)
    except ValueError:
        return None
    following = arguments[index + 1 : index + 2]
    return following[0] if following else None


def _seconds(duration: str) -> int:
    """Convert a nextest duration such as ``900s`` or ``30m`` to seconds."""
    return int(duration[:-1]) * DURATION_UNITS[duration[-1]]


def _terminate_after(slow_timeout: dict[str, typ.Any]) -> int:
    """How many slow periods a test may run through before it is killed.

    nextest reads a missing or zero value as "never terminate": the period
    expires, the test is reported as slow, and it keeps running until
    something else stops it.
    """
    return int(slow_timeout.get("terminate-after", 0))


def _kill_point(slow_timeout: dict[str, typ.Any]) -> int | None:
    """The second at which a slow test is killed, or `None` if it never is.

    `terminate-after` counts periods rather than seconds, so a timeout
    declaring two periods acts at twice the period it advertises.
    """
    periods = _terminate_after(slow_timeout)
    return _seconds(slow_timeout["period"]) * periods if periods > 0 else None


def _ui_routes(
    config: dict[str, typ.Any],
) -> list[tuple[str, dict[str, typ.Any]]]:
    """The slow timeouts that can govern the UI package, labelled by route."""
    routes = [(PROFILE, config["profile"][PROFILE]["slow-timeout"])]
    routes.extend(
        (f"override {index}", override["slow-timeout"])
        for index, override in enumerate(_package_overrides(config))
    )
    return routes


def test_the_make_target_passes_the_profile_to_nextest(
    tmp_path: pathlib.Path,
) -> None:
    """`make test-rstest-bdd-ui` runs nextest *under* the profile.

    The profile is read from the environment of the process Make actually
    started, so this fails when the assignment never reaches `nextest` - for
    instance if `NEXTEST_PROFILE=` were moved onto its own recipe line, where
    Make would run it as a separate shell and the runner would inherit
    nothing.
    """
    invocation = _invoke_target(tmp_path)
    assert invocation.arguments[:2] == ("nextest", "run"), invocation
    assert invocation.profile == PROFILE, invocation
    assert _option_value(invocation.arguments, "-p") == PACKAGE, invocation


def test_the_ui_profile_raises_the_run_wide_timeout() -> None:
    """The profile exists, and raises the ceiling the override cannot reach.

    The per-package override only replaces `slow-timeout`. Without a profile
    carrying a larger `global-timeout`, a cold trybuild build can still be cut
    off by the default 10-minute run ceiling.
    """
    profile = _nextest_config()["profile"][PROFILE]
    assert profile["global-timeout"] == "30m", profile
    assert profile["slow-timeout"]["period"] == "900s", profile["slow-timeout"]


def test_the_package_override_matches_the_profile() -> None:
    """The default-profile override keeps the same generous slow timeout.

    A run that reaches the package without selecting the profile should still
    treat a slow trybuild build as slow rather than failed at 300 seconds.
    """
    matching = _package_overrides(_nextest_config())
    assert matching, f"no override for {PACKAGE}"
    for override in matching:
        assert override["slow-timeout"]["period"] == "900s", override


def test_the_profile_and_override_share_one_budget() -> None:
    """Both routes into the package allow the same slow timeout.

    Whichever way a run reaches `rstest-bdd-ui-tests`, the ceiling it is held
    to comes from the same number, so the two cannot drift apart.
    """
    config = _nextest_config()
    profile_period = config["profile"][PROFILE]["slow-timeout"]["period"]
    override_periods = {
        override["slow-timeout"]["period"] for override in _package_overrides(config)
    }
    assert override_periods == {profile_period}, override_periods


def test_the_run_ceiling_covers_the_slow_timeout() -> None:
    """The selected profile leaves room for the override it enables.

    A raised slow timeout only helps while the run-wide ceiling sits above it:
    below that, `terminate-after` never fires and the whole run is killed
    instead. Each step in the chain therefore has to exceed the one it
    replaces, and the profile's ceiling has to beat the default one rather than
    merely restate a large number.
    """
    config = _nextest_config()
    default = config["profile"]["default"]
    profile = config["profile"][PROFILE]
    overrides = _package_overrides(config)
    assert overrides, f"no override for {PACKAGE}"
    default_slow = _seconds(default["slow-timeout"]["period"])
    default_global = _seconds(default["global-timeout"])
    override_slow = _seconds(overrides[0]["slow-timeout"]["period"])
    profile_slow = _seconds(profile["slow-timeout"]["period"])
    profile_global = _seconds(profile["global-timeout"])
    assert default_slow < override_slow, (default_slow, override_slow)
    assert default_slow < profile_slow, (default_slow, profile_slow)
    assert default_global < profile_global, (default_global, profile_global)
    assert override_slow < profile_global, (override_slow, profile_global)


def test_the_ui_routes_terminate_a_test_that_overruns() -> None:
    """A slow UI test is stopped, at one period, on both routes into it.

    A period on its own only reports a test as slow; without `terminate-after`
    the test keeps running until the run-wide ceiling kills the entire run,
    which is what the raised ceiling exists to postpone. The kill point is
    asserted from the period and the count together, because a timeout of two
    periods would advertise a period it does not act on for twice as long.
    """
    routes = _ui_routes(_nextest_config())
    assert routes, f"no route into {PACKAGE}"
    for name, slow_timeout in routes:
        assert _terminate_after(slow_timeout) == 1, (name, slow_timeout)
        assert _kill_point(slow_timeout) == 900, (name, slow_timeout)
        assert slow_timeout["grace-period"] == "5s", (name, slow_timeout)


def test_the_default_route_terminates_a_test_that_overruns() -> None:
    """The fallback route stops an overrunning test rather than only warning.

    This is the route every other package takes, and the one the UI package
    may reach without the profile, so it has to terminate on its own terms
    rather than relying on a ceiling far above it.
    """
    default = _nextest_config()["profile"]["default"]["slow-timeout"]
    assert _terminate_after(default) == 1, default
    assert _kill_point(default) == 60, default


def _workflow_steps() -> list[dict[str, typ.Any]]:
    """Every step of the CI workflow, flattened across its jobs."""
    workflow = yaml.safe_load(
        (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    )
    return [
        step for job in workflow["jobs"].values() for step in job.get("steps") or []
    ]


def test_the_behavioural_contracts_run_where_nextest_is_installed() -> None:
    """The real-nextest proof runs in the lane that has a Rust build.

    `test_rstest_bdd_ui_timeouts.py` skips when nextest is unavailable, which
    is what keeps `make test-workflow-contracts` fast and Rust-free in the
    docs-tooling lane. That skip is only safe while something asserts the
    tests run somewhere: without this contract, deleting the CI step would
    leave every behavioural check skipped and the suite still green.
    """
    steps = _workflow_steps()
    commands = [step.get("run", "") for step in steps]
    assert any("make test-rstest-bdd-ui-timeouts" in run for run in commands), commands
    assert any("make test-workflow-contracts" in run for run in commands), (
        "the fast contracts must still run somewhere"
    )


def test_the_behavioural_target_refuses_to_skip_without_nextest() -> None:
    """The proof target fails loudly rather than skipping on a weak lane.

    The suite-level skip protects the docs-tooling lane; the target exists to
    stop that protection from covering a lane that was meant to have nextest.
    """
    recipe = (REPO_ROOT / "Makefile").read_text(encoding="utf-8")
    target = recipe.split("test-rstest-bdd-ui-timeouts:", 1)
    assert len(target) == 2, "the behavioural target is missing"
    body = target[1].split("\n\n", 1)[0]
    assert "nextest --version" in body, body
    assert "exit 1" in body, body


def test_the_docs_tooling_lane_stays_free_of_a_rust_build() -> None:
    """The fast contracts keep their lane cheap, which is why they skip.

    If this lane ever gains nextest, the skip becomes pointless and the split
    between the two files should be revisited - so the assumption is recorded
    here rather than left implicit in a skipif reason.
    """
    workflow = yaml.safe_load(
        (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    )
    lane = workflow["jobs"]["docs-tooling"]
    assert all("nextest" not in (step.get("run") or "") for step in lane["steps"])
