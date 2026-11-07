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
import re
import subprocess
import tomllib
import typing as typ

import pytest
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

# nextest parses durations with a unit grammar whose units are words, not
# single letters: `900ms` ends in the same character as `900s`. The number and
# the unit are therefore split apart and the unit looked up whole, so a
# millisecond budget cannot be read as a second one.
DURATION_PATTERN = re.compile(r"^(?P<count>\d+)(?P<unit>[A-Za-zµ]+)$")

# The units these contracts need to compare one budget against another.
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
    """Convert a nextest duration such as ``900s`` or ``30m`` to seconds.

    A value this table cannot read fails here rather than in a comparison
    downstream, and names itself, so a configuration edit reports the budget it
    wrote instead of a `KeyError` or an `int()` failure on its first character.

    >>> _seconds("30m")
    1800
    """
    match = DURATION_PATTERN.match(duration)
    unit = DURATION_UNITS.get(match["unit"]) if match else None
    if match is None or unit is None:
        message = f"unsupported nextest duration: {duration!r}"
        raise ValueError(message)
    return int(match["count"]) * unit


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


def test_an_unsupported_duration_names_the_value_it_rejected() -> None:
    """A value this table cannot read is reported as itself.

    Every assertion above subtracts one budget from another, so a duration the
    table lacks would surface as a `KeyError` on a single character, far from
    the configuration line that caused it. The message has to name the duration
    for the failure to point at its own cause.
    """
    with pytest.raises(ValueError, match="unsupported nextest duration: '900ms'"):
        _seconds("900ms")


def test_a_multi_character_unit_is_split_off_whole() -> None:
    """A unit longer than one character is read as a unit, not as a suffix.

    `900ms` ends in the same letter as `900s`, so reading the last character
    alone would treat a millisecond budget as a second one and then fail on the
    leftover `m`. The split has to reach the end of the number, which a
    single-character pattern would not.
    """
    match = DURATION_PATTERN.match("900ms")
    assert match is not None, "the duration pattern rejected 900ms"
    assert match["count"] == "900", match["count"]
    assert match["unit"] == "ms", match["unit"]


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


def _steps_running(needle: str) -> list[tuple[str, dict[str, typ.Any]]]:
    """Every (job name, step) whose `run` text contains `needle`."""
    workflow = yaml.safe_load(
        (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    )
    return [
        (job_name, step)
        for job_name, job in workflow["jobs"].items()
        for step in job.get("steps") or []
        if needle in (step.get("run") or "")
    ]


def test_the_behavioural_contracts_run_where_nextest_is_installed() -> None:
    """The real-nextest proof runs once, in the lane that has a Rust build.

    `make test-workflow-contracts` excludes `test_rstest_bdd_ui_timeouts.py`,
    because the fixtures take minutes and the docs-tooling lane has no nextest.
    So the dedicated target is the only thing that runs those contracts, and
    deleting its step would leave them run by nothing at all while the suite
    stayed green.

    Which job, and which leg, matters as much as that the step exists. On the
    matrix legs the target would run three times over, and it needs nextest and
    a Rust build, which the docs-tooling job has neither of; a step that moved
    there would be skipped or would fail for the wrong reason. The `if`
    condition is asserted as text because that is what the workflow carries,
    and a mutation deleting it is exactly the regression this covers.
    """
    matches = _steps_running("make test-rstest-bdd-ui-timeouts")
    assert len(matches) == 1, matches
    job_name, step = matches[0]
    assert job_name == "build-test", job_name
    assert step.get("if") == "matrix.name == 'wireframe-only'", step

    fast = _steps_running("make test-workflow-contracts")
    assert fast, "the fast contracts must still run somewhere"


def test_the_fast_target_does_not_run_the_behavioural_contracts() -> None:
    """The cheap target excludes the file the dedicated target owns.

    Both targets run pytest over `tests/workflow_contracts`, so without the
    exclusion `make test-workflow-contracts` picks up the behavioural file on
    any machine that has nextest. It would then take minutes rather than
    seconds and require a Rust build, in the lane chosen precisely because it
    needs neither. The assertion is on the pytest invocation, not the target
    as a whole, so the ruff checks above it stay free to lint the file.
    """
    recipe = (REPO_ROOT / "Makefile").read_text(encoding="utf-8")
    body = recipe.split("test-workflow-contracts:", 1)[1].split("\n\n", 1)[0]
    assert "--ignore=tests/workflow_contracts/test_rstest_bdd_ui_timeouts.py" in body, (
        body
    )
    assert "python -m pytest tests/workflow_contracts" in body, body


def test_the_behavioural_target_refuses_to_skip_without_nextest() -> None:
    """The proof target fails loudly rather than skipping on a weak lane.

    The suite-level skip covers a direct `pytest` run; the target exists to stop
    that protection from covering a lane that was meant to have nextest. It
    probes the same `$(CARGO)` the fixtures build with, so a machine where one
    Cargo has nextest and another does not cannot pass the probe and then skip
    every contract.
    """
    recipe = (REPO_ROOT / "Makefile").read_text(encoding="utf-8")
    target = recipe.split("test-rstest-bdd-ui-timeouts:", 1)
    assert len(target) == 2, "the behavioural target is missing"
    body = target[1].split("\n\n", 1)[0]
    assert "$(CARGO) nextest --version" in body, body
    assert "exit 1" in body, body
    assert "|| { " in body or "|| {" in body, body


def test_the_docs_tooling_lane_stays_free_of_a_rust_build() -> None:
    """The fast lane stays cheap, and the behavioural file never lands in it.

    If this lane ever gained nextest, excluding the behavioural file from
    `make test-workflow-contracts` would stop buying anything and the split
    between the two files should be revisited - so the assumption is recorded
    here rather than left implicit in a Makefile comment.

    The second assertion is the one the cheap lane actually depends on. The
    first alone only catches a step whose text mentions `nextest`; the target
    that runs the behavioural contracts does not, so a step invoking it here
    would sail past a `nextest` grep and then either skip or need a Rust build.
    """
    workflow = yaml.safe_load(
        (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    )
    lane = workflow["jobs"]["docs-tooling"]
    runs = [step.get("run") or "" for step in lane["steps"]]
    assert all("nextest" not in run for run in runs), runs
    assert all("test-rstest-bdd-ui-timeouts" not in run for run in runs), runs
