"""Real-nextest proof that the UI gate's time budget is enforced, not just set.

The contracts in `test_rstest_bdd_ui_gate.py` read `.config/nextest.toml` and
run the Make target under a Cargo stub. Both are worth having, but neither can
show that nextest acts on the numbers: libtest ignores the file entirely, so a
`cargo test` run of the UI package would pass whatever the budgets say, and a
stub records the profile without ever applying it.

`terminate-after` is the setting that makes the difference visible. Without it a
period only reports a test as slow (`SLOW [>  3.000s]`) and the test keeps
running; with it, nextest kills the test and reports `TIMEOUT`. When neither
applies, the run-wide ceiling eventually kills the whole run and reports
`SIGTERM`. Those three words are the outcome *and* the reason, so the contracts
here assert on them rather than on elapsed time alone.

The fixture is disposable: two member crates with no dependencies, built into
the repository's `target` directory, and a `.config/nextest.toml` whose budgets
are seconds rather than minutes. One crate matches a package override and one
deliberately does not. They are run separately rather than together, so each
outcome is attributed to the crate that produced it: the matching crate shows a
per-package timeout, the control shows the default route's fall-through to the
run ceiling.

The `rstest-bdd-ui` profile is declared in the fixture with seconds-scale
budgets, and a third group of contracts selects it by name. Reading the file
proves the budgets are written down; only a run proves nextest compiles that
profile, selects it, and enforces it. Nothing here touches the real UI package
or the shared compile-admission queue: the fixture has no dependencies, so its
build is a single leaf crate per member.
"""

from __future__ import annotations

import dataclasses
import os
import pathlib
import re
import subprocess
import tomllib
import typing as typ

import pytest

from ci_workflow_reader import REPO_ROOT

OVERRIDDEN = "crate-overridden"
CONTROL = "crate-control"
TEST_NAME = "overruns"

# The profile `make test-rstest-bdd-ui` selects. `_production_profile_name`
# reads the same name back out of the Makefile, so this literal cannot drift
# from the target it stands for without a contract failing.
PROFILE = "rstest-bdd-ui"

# The fixture's budgets, in seconds. They are short enough to keep the lane
# cheap and far enough apart that ordering, not a tight window, is what the
# contracts assert: the override must fire before the run ceiling, and the
# control must reach the ceiling without ever being terminated per-test.
OVERRIDE_PERIOD = 3
DEFAULT_PERIOD = 2
GLOBAL_TIMEOUT = 6
TEST_SLEEP = 60

# The named profile as the fixture normally declares it. Its period is longer
# than `DEFAULT_PERIOD`, so a run that selected the default profile by mistake
# would report `SLOW` where this one reports `TIMEOUT`: the outcome word, not
# the elapsed time alone, is what discriminates.
PROFILE_PERIOD = 5
PROFILE_GLOBAL = 20

# A second shape, where the profile's ceiling is shorter than its period and
# shorter than the default ceiling. The period cannot fire before the ceiling,
# so `SLOW` cannot be printed at all - its absence is structural rather than a
# race - and the only reachable outcome is `SIGTERM` at the profile's ceiling.
TIGHT_PROFILE_PERIOD = 30
TIGHT_PROFILE_GLOBAL = 2

ANSI = re.compile(r"\x1b\[[0-9;]*m")
RESULT_LINE = re.compile(
    r"^\s*(?P<kind>TIMEOUT|SIGTERM|SLOW|PASS|FAIL)"
    r".*?\[[>\s]*(?P<seconds>\d+(?:\.\d+)?)s\]"
    r".*?(?P<package>crate-[a-z]+)\s+(?P<test>tests::\w+)"
)

# nextest announces the profile it resolved on its banner line. Reading it back
# is how a contract distinguishes "the named profile ran" from "a profile I
# configured ran" - the numbers alone cannot tell the two apart.
PROFILE_LINE = re.compile(r"with nextest profile:\s*(?P<profile>[\w-]+)")


@dataclasses.dataclass(frozen=True)
class Outcome:
    """One result line, as nextest reported it."""

    kind: str
    seconds: float
    package: str


@dataclasses.dataclass(frozen=True)
class Run:
    """What one nextest invocation reported: the profile it resolved, its results."""

    profile: str
    outcomes: list[Outcome]


def _nextest_is_available() -> bool:
    """Whether a real `cargo nextest` can be run in this environment.

    The lane that runs `make test-workflow-contracts` has no Rust build of this
    workspace and does not install nextest, so the behavioural contracts skip
    there. They run in the lane that does; `test_rstest_bdd_ui_gate.py` pins
    that wiring so the skip cannot quietly become permanent.
    """
    try:
        probe = subprocess.run(  # noqa: S603 - fixed argv, no shell
            ["cargo", "nextest", "--version"],  # noqa: S607
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return False
    return probe.returncode == 0


pytestmark = pytest.mark.skipif(
    not _nextest_is_available(),
    reason="a real cargo nextest is required for the behavioural contracts",
)


def _member(directory: pathlib.Path, name: str) -> None:
    """Write one fixture member whose test overruns every budget."""
    (directory / "src").mkdir(parents=True)
    (directory / "Cargo.toml").write_text(
        f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2024"\n',
        encoding="utf-8",
    )
    (directory / "src" / "lib.rs").write_text(
        "//! Fixture member for the timeout contracts.\n\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    /// Sleeps past every budget the fixture configures.\n"
        "    #[test]\n"
        f"    fn {TEST_NAME}() {{\n"
        f"        std::thread::sleep(std::time::Duration::from_secs({TEST_SLEEP}));\n"
        "    }\n"
        "}\n",
        encoding="utf-8",
    )


def _config(
    terminate: bool,
    profile_period: int = PROFILE_PERIOD,
    profile_global: int = PROFILE_GLOBAL,
) -> str:
    """The fixture's nextest configuration.

    `terminate` decides whether the overridden package's period kills the test.
    Turning it off is how the "slow warning" outcome is produced without
    changing anything else.

    The `rstest-bdd-ui` profile is declared here with second-scale budgets, in
    the same shape production declares it, so the behavioural contracts can
    select the profile *name* production selects. A profile that is absent, or
    whose tables do not parse, is an error nextest reports at startup; a profile
    that parses but is never selected silently falls back to `default`. Only a
    run covers both.
    """
    override = (
        f'slow-timeout = {{ period = "{OVERRIDE_PERIOD}s", '
        'terminate-after = 1, grace-period = "1s" }'
        if terminate
        else f'slow-timeout = {{ period = "{OVERRIDE_PERIOD}s", grace-period = "1s" }}'
    )
    return (
        "[profile.default]\n"
        f'slow-timeout = {{ period = "{DEFAULT_PERIOD}s", grace-period = "1s" }}\n'
        f'global-timeout = "{GLOBAL_TIMEOUT}s"\n'
        "\n"
        "[[profile.default.overrides]]\n"
        f'filter = "package({OVERRIDDEN})"\n'
        f"{override}\n"
        "\n"
        f"[profile.{PROFILE}]\n"
        f'slow-timeout = {{ period = "{profile_period}s", '
        'terminate-after = 1, grace-period = "1s" }\n'
        f'global-timeout = "{profile_global}s"\n'
    )


def _workspace(
    root: pathlib.Path,
    terminate: bool,
    profile_period: int = PROFILE_PERIOD,
    profile_global: int = PROFILE_GLOBAL,
) -> pathlib.Path:
    """Create the two-member fixture workspace and return its path."""
    root.mkdir(parents=True, exist_ok=True)
    (root / "Cargo.toml").write_text(
        f'[workspace]\nresolver = "3"\nmembers = ["{OVERRIDDEN}", "{CONTROL}"]\n',
        encoding="utf-8",
    )
    _member(root / OVERRIDDEN, OVERRIDDEN)
    _member(root / CONTROL, CONTROL)
    (root / ".config").mkdir()
    (root / ".config" / "nextest.toml").write_text(
        _config(terminate, profile_period, profile_global), encoding="utf-8"
    )
    return root


def _fixture_env(profile: str | None = None) -> dict[str, str]:
    """The environment the fixture is built and run in.

    The lane's build flags are removed rather than inherited. They are tuned for
    this workspace - CI exports nightly-only `-Z` flags, and `make lint` carries
    `-D warnings` - and the fixture is a plain leaf crate that must build under
    whatever toolchain the lane has. Inheriting them made `cargo metadata` fail
    before nextest ever ran, which is a harness failure, not a timeout result.

    `NEXTEST_PROFILE` is set rather than inherited, for the same reason: the
    contracts below are decided by which profile governs, so an ambient value
    left over from a surrounding run must not choose it. `None` selects
    `default` explicitly, which keeps the default-route contracts running the
    route they describe.

    `RUSTC_WRAPPER` is deliberately kept: compile admission is not bypassed.
    """
    env = {
        key: value
        for key, value in os.environ.items()
        if key
        not in {
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "NEXTEST_PROFILE",
        }
    }
    env["CARGO_TARGET_DIR"] = str(REPO_ROOT / "target" / "timeout-contract-fixtures")
    env["NEXTEST_PROFILE"] = profile or "default"
    return env


def _run(root: pathlib.Path, package: str, profile: str | None = None) -> Run:
    """Run one fixture member under the fixture's nextest configuration.

    `profile` is the nextest profile name to select, or `None` for `default`.
    Passing it through `NEXTEST_PROFILE` rather than a command-line flag is
    what the Make target does, so the fixture selects its profile the same way
    production does. The package is scoped with `-p` for the same reason, and
    because a test-name filter alone would build both members for every
    contract and double the time spent in compile admission.

    A failing exit status is expected and ignored: the point of the run is what
    nextest reports, and a timed-out test exits non-zero by design. What is not
    ignored is a run that reports nothing parseable, because that is a broken
    harness rather than a result - the raw output goes into the failure so the
    cause is readable instead of the empty list it would otherwise produce.
    """
    completed = subprocess.run(  # noqa: S603 - fixed argv, no shell
        [  # noqa: S607
            "cargo",
            "nextest",
            "run",
            "-p",
            package,
            "-E",
            f"test({TEST_NAME})",
        ],
        cwd=root,
        capture_output=True,
        text=True,
        timeout=600,
        check=False,
        env=_fixture_env(profile),
    )
    output = ANSI.sub("", completed.stdout + completed.stderr)
    outcomes = _outcomes(output)
    if not outcomes:
        raise AssertionError(
            f"nextest reported no result for {package} in {root} "
            f"under profile {profile or 'default'} "
            f"(exit {completed.returncode}):\n{output[-4000:]}"
        )
    return Run(_resolved_profile(output, profile), outcomes)


def _resolved_profile(output: str, requested: str | None) -> str:
    """The profile nextest says it used, falling back to the one requested.

    The banner is the only place nextest states its choice. Older versions do
    not print it, so the requested name stands in; the outcome contracts below
    still discriminate through the budgets, which is the stronger evidence
    either way.
    """
    match = PROFILE_LINE.search(output)
    return match["profile"] if match is not None else requested or "default"


def _outcomes(output: str) -> list[Outcome]:
    """Every result line in nextest's output, in the order it printed them."""
    outcomes: list[Outcome] = []
    for line in ANSI.sub("", output).splitlines():
        match = RESULT_LINE.match(line)
        if match is None:
            continue
        outcomes.append(
            Outcome(
                match["kind"],
                float(match["seconds"]),
                match["package"],
            )
        )
    return outcomes


def _kinds(outcomes: list[Outcome]) -> list[str]:
    """The result kinds reported for one fixture member."""
    return [outcome.kind for outcome in outcomes]


def _last_kind(outcomes: list[Outcome]) -> str:
    """The final result nextest reported for the fixture member."""
    assert outcomes, "nextest reported no result for the fixture member"
    return outcomes[-1].kind


def _first_time(outcomes: list[Outcome], kind: str) -> float:
    """When nextest reported `kind`, as the contract's ordering evidence."""
    matching = [outcome.seconds for outcome in outcomes if outcome.kind == kind]
    assert matching, f"nextest never reported {kind}: {_kinds(outcomes)}"
    return matching[0]


def test_an_overrunning_test_is_terminated_by_the_package_override(
    tmp_path: pathlib.Path,
) -> None:
    """The override kills the test it matches, at its own period.

    `TIMEOUT` is nextest's word for a per-test termination, so this fails if
    `terminate-after` is dropped: the same test then runs on and the run
    reports `SIGTERM` at the ceiling instead.
    """
    outcomes = _run(
        _workspace(tmp_path / "terminating", terminate=True), OVERRIDDEN
    ).outcomes
    assert _last_kind(outcomes) == "TIMEOUT", _kinds(outcomes)
    assert "SIGTERM" not in _kinds(outcomes), _kinds(outcomes)


def test_a_period_without_terminate_after_only_warns(
    tmp_path: pathlib.Path,
) -> None:
    """A period alone reports the test as slow and never stops it.

    This is the distinction the setting exists for. Same crate, same period,
    same sleep: only `terminate-after` changed, and nextest stops saying
    `TIMEOUT` at the period and instead warns `SLOW` until the run ceiling
    kills the whole run with `SIGTERM`. One `SLOW` is all that fits before the
    ceiling here, which is the point: the period no longer acts on its own.
    """
    outcomes = _run(
        _workspace(tmp_path / "warning", terminate=False), OVERRIDDEN
    ).outcomes
    kinds = _kinds(outcomes)
    assert "TIMEOUT" not in kinds, kinds
    assert "SLOW" in kinds, kinds
    assert _last_kind(outcomes) == "SIGTERM", kinds


def test_a_non_matching_package_falls_through_to_the_run_ceiling(
    tmp_path: pathlib.Path,
) -> None:
    """An override does not leak to a package it does not match.

    The control crate is over budget for the default period but matches no
    override, so it warns and then meets the run-wide ceiling. A filter that
    matched too broadly would terminate it early, and this would report
    `TIMEOUT` rather than `SIGTERM`.
    """
    outcomes = _run(_workspace(tmp_path / "control", terminate=True), CONTROL).outcomes
    assert "TIMEOUT" not in _kinds(outcomes), _kinds(outcomes)
    assert _last_kind(outcomes) == "SIGTERM", _kinds(outcomes)


def test_the_override_fires_before_the_run_ceiling(tmp_path: pathlib.Path) -> None:
    """The two budgets act in the order their values imply.

    Ordering rather than a fixed window is what is asserted: a loaded host can
    delay either report, but it cannot make the shorter budget act later than
    the longer one.
    """
    root = _workspace(tmp_path / "ordering", terminate=True)
    overridden = _first_time(_run(root, OVERRIDDEN).outcomes, "TIMEOUT")
    control = _first_time(_run(root, CONTROL).outcomes, "SIGTERM")
    assert overridden < control, (overridden, control)


def _production_overrides() -> list[dict[str, typ.Any]]:
    """Every package override in the repository's own nextest configuration."""
    config = tomllib.loads(
        (REPO_ROOT / ".config" / "nextest.toml").read_text(encoding="utf-8")
    )
    return config["profile"]["default"].get("overrides") or []


def test_the_fixture_ignores_the_lane_build_flags(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The fixture's build is insulated from the lane's `RUSTFLAGS`.

    CI exports nightly-only `-Z` flags for this workspace, so inheriting them
    made `cargo metadata` fail before nextest ran and every behavioural
    contract reported an empty result. The wrapper is kept, so compile
    admission still applies.
    """
    monkeypatch.setenv("RUSTFLAGS", "-D warnings -Zthreads=8")
    monkeypatch.setenv("CARGO_ENCODED_RUSTFLAGS", "-Dwarnings")
    monkeypatch.setenv("RUSTC_WRAPPER", "/usr/bin/compile-admission")
    env = _fixture_env()
    assert "RUSTFLAGS" not in env, env
    assert "CARGO_ENCODED_RUSTFLAGS" not in env, env
    assert env["RUSTC_WRAPPER"] == "/usr/bin/compile-admission", env
    assert env["CARGO_TARGET_DIR"].endswith("timeout-contract-fixtures"), env


def test_the_fixture_matches_the_shape_production_uses() -> None:
    """The fixture is overridden by package filter, as production is.

    Without this the contracts above would keep passing while production moved
    to a filter shape the fixture never exercises - a test-name filter, say,
    under which the fixture's override would not match and the `TIMEOUT` it
    asserts would never appear.
    """
    production_filters = [override["filter"] for override in _production_overrides()]
    assert all("package(" in f for f in production_filters), production_filters
    assert f'filter = "package({OVERRIDDEN})"' in _config(terminate=True)


def test_the_profile_name_is_the_one_the_make_target_selects() -> None:
    """The fixture's profile name is read out of the target, not guessed.

    The named-profile contracts below are only evidence about `make
    test-rstest-bdd-ui` if they select the same profile that target selects. A
    rename on either side would otherwise leave them passing against a profile
    the Makefile no longer uses.
    """
    makefile = (REPO_ROOT / "Makefile").read_text(encoding="utf-8")
    target = makefile.split("\ntest-rstest-bdd-ui:", 1)[1].split("\n\n", 1)[0]
    assert f"NEXTEST_PROFILE={PROFILE} " in target, target
    production = tomllib.loads(
        (REPO_ROOT / ".config" / "nextest.toml").read_text(encoding="utf-8")
    )
    assert PROFILE in production["profile"], production["profile"]
    assert f"[profile.{PROFILE}]" in _config(terminate=True)


def test_the_named_profile_selects_and_enforces_its_own_period(
    tmp_path: pathlib.Path,
) -> None:
    """A run under the named profile reports `TIMEOUT` at that profile's period.

    Two things are asserted, and they are different things: nextest announced
    the profile by name, and the outcome is the one only that profile's budget
    produces. The fixture's named period is longer than the default period, so
    a run that fell back to `default` would report `SLOW` and then `SIGTERM`
    instead of `TIMEOUT`.

    This is the link the file-reading contracts cannot supply. They prove the
    budgets are written; this proves nextest compiles the profile, selects it,
    and acts on it.
    """
    run = _run(_workspace(tmp_path / "named", terminate=True), CONTROL, profile=PROFILE)
    assert run.profile == PROFILE, run.profile
    assert _last_kind(run.outcomes) == "TIMEOUT", _kinds(run.outcomes)
    assert _first_time(run.outcomes, "TIMEOUT") >= PROFILE_PERIOD - 1, run.outcomes


def test_the_named_profile_ceiling_stops_the_run_when_it_comes_first(
    tmp_path: pathlib.Path,
) -> None:
    """The named profile's own `global-timeout` governs its run.

    With a ceiling shorter than its period, the period cannot fire first, so
    `SLOW` is unreachable rather than merely unlikely and its absence is strong
    evidence. The ceiling is also three times shorter than the default one, so
    reaching it this early can only mean the named profile's value was in force.
    """
    run = _run(
        _workspace(
            tmp_path / "named-ceiling",
            terminate=True,
            profile_period=TIGHT_PROFILE_PERIOD,
            profile_global=TIGHT_PROFILE_GLOBAL,
        ),
        CONTROL,
        profile=PROFILE,
    )
    assert run.profile == PROFILE, run.profile
    assert "SLOW" not in _kinds(run.outcomes), _kinds(run.outcomes)
    assert "TIMEOUT" not in _kinds(run.outcomes), _kinds(run.outcomes)
    assert _last_kind(run.outcomes) == "SIGTERM", _kinds(run.outcomes)
    assert _first_time(run.outcomes, "SIGTERM") < GLOBAL_TIMEOUT, run.outcomes


def test_the_default_route_is_what_the_fixture_selects_when_asked(
    tmp_path: pathlib.Path,
) -> None:
    """The unnamed route really is `default`, so the comparison above is sound.

    The named-profile contract above argues from a difference: the same crate
    under `default` behaves differently. That argument needs the difference to
    exist, so this pins the other half - `default` is selected, and its shorter
    period warns instead of terminating.
    """
    run = _run(_workspace(tmp_path / "unnamed", terminate=True), CONTROL)
    assert run.profile == "default", run.profile
    assert "TIMEOUT" not in _kinds(run.outcomes), _kinds(run.outcomes)
    assert "SLOW" in _kinds(run.outcomes), _kinds(run.outcomes)
    assert _last_kind(run.outcomes) == "SIGTERM", _kinds(run.outcomes)
