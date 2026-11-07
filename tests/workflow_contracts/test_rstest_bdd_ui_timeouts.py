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
profile, selects it, and enforces it. A final group runs the shipped
`test-rstest-bdd-ui` recipe itself against a fixture, so what is exercised is
the target production runs rather than an equivalent command written here.
Nothing here touches the real UI package or the shared compile-admission
queue: the fixtures have no dependencies, so each build is a single leaf crate.
"""

from __future__ import annotations

import dataclasses
import os
import pathlib
import re
import shutil
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

# The end-to-end fixture drives the real Make target, which hardcodes this
# package name. The budgets follow the same pattern as above: a named profile
# that terminates its test and a default route that does not, so a run that
# failed to select the named profile fails fast with `SIGTERM` rather than
# hanging until the harness gives up.
UI_PACKAGE = "rstest-bdd-ui-tests"
END_TO_END_PERIOD = 3
END_TO_END_GLOBAL = 40

ANSI = re.compile(r"\x1b\[[0-9;]*m")
RESULT_LINE = re.compile(
    r"^\s*(?P<kind>TIMEOUT|SIGTERM|SLOW|PASS|FAIL)"
    r".*?\[[>\s]*(?P<seconds>\d+(?:\.\d+)?)s\]"
    r".*?(?P<package>[a-z][a-z0-9-]*)\s+(?P<test>tests::\w+)"
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
    (directory / "src").mkdir(parents=True, exist_ok=True)
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


def _end_to_end_workspace(root: pathlib.Path, with_ceiling: bool) -> pathlib.Path:
    """Create a fixture workspace holding the package the Make target names.

    The target hardcodes `-p rstest-bdd-ui-tests`, so the fixture has to supply
    a package by that name or the target would fail before nextest ran. The
    fixture is deliberately not the real package: it is one leaf crate with no
    dependencies, so the end-to-end contract costs a single compile rather than
    a build of the UI gate's trybuild harness.

    `with_ceiling` chooses which half under test. With it, the profile's
    ceiling is the shortest budget in the file, so reaching it can only be the
    named profile governing. Without it, the ceiling is long enough that the
    period fires first, and the profile's own `terminate-after` is what stops
    the test.

    The same directory is reused across runs, so every write is idempotent: a
    second run must reproduce the fixture rather than fail on a leftover one.
    """
    root.mkdir(parents=True, exist_ok=True)
    (root / "Cargo.toml").write_text(
        f'[workspace]\nresolver = "3"\nmembers = ["{UI_PACKAGE}"]\n',
        encoding="utf-8",
    )
    _member(root / UI_PACKAGE, UI_PACKAGE)
    (root / ".config").mkdir(exist_ok=True)
    if with_ceiling:
        default_ceiling, profile_period, profile_ceiling = 8, 30, 2
    else:
        default_ceiling, profile_period, profile_ceiling = 8, END_TO_END_PERIOD, 40
    (root / ".config" / "nextest.toml").write_text(
        "[profile.default]\n"
        f'slow-timeout = {{ period = "{DEFAULT_PERIOD}s", grace-period = "1s" }}\n'
        f'global-timeout = "{default_ceiling}s"\n'
        "\n"
        f"[profile.{PROFILE}]\n"
        f'slow-timeout = {{ period = "{profile_period}s", '
        'terminate-after = 1, grace-period = "1s" }\n'
        f'global-timeout = "{profile_ceiling}s"\n',
        encoding="utf-8",
    )
    return root


def _end_to_end_env() -> dict[str, str]:
    """The environment the real Make target is run in.

    Unlike `_fixture_env`, the target's own flags are left alone: this contract
    exists to run what production runs, and the recipe's `RUSTFLAGS` and
    `NEXTEST_PROFILE` are part of that. Only the environment that would move
    the run somewhere else is pinned.

    `CARGO_TARGET_DIR` is shared with the other fixtures so the build cache is
    reused. `NEXTEST_PROFILE` is removed rather than set: the recipe assigns it
    itself, and inheriting one would mask a recipe that stopped assigning it.
    """
    env = {
        key: value
        for key, value in os.environ.items()
        if key not in {"NEXTEST_PROFILE", "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET"}
    }
    env["CARGO_TARGET_DIR"] = str(REPO_ROOT / "target" / "timeout-contract-fixtures")
    return env


def _write_lockfile(root: pathlib.Path) -> None:
    """Give the fixture workspace the lockfile a real checkout would have.

    `test-rstest-bdd-ui` runs after the `fetch` prerequisite, which is
    `cargo fetch --locked`. A committed `Cargo.lock` is part of what that
    recipe assumes: it is how the suite gets a reproducible dependency set
    offline. The fixture is generated rather than checked in, so it starts
    without one and `--locked` refuses to create it, failing the instruction's
    intent for a reason that has nothing to do with timeouts.

    Cargo writes the file itself, so the format stays whatever the toolchain
    produces rather than something pinned here that would drift from it.
    """
    completed = subprocess.run(  # noqa: S603 - fixed argv, no shell
        [shutil.which("cargo"), "generate-lockfile", "--offline"],
        cwd=root,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
        env=_end_to_end_env(),
    )
    if completed.returncode != 0:
        message = f"could not write the fixture lockfile:\n{completed.stderr}"
        raise RuntimeError(message)


def _make_target(root: pathlib.Path) -> tuple[str, int]:
    """Run the real `test-rstest-bdd-ui` recipe in `root`, and report its output.

    `make -f <repo>/Makefile` with a fixture working directory is what makes
    this end-to-end: the recipe that runs is the shipped one, flags and
    profile assignment included, and only the workspace it compiles belongs to
    the fixture. `CARGO` is pinned to the real binary because the Makefile
    prepends `$(HOME)/.cargo/bin` to `PATH`, which would otherwise shadow any
    stub.

    The fixture lives under the repository's `target` directory on purpose.
    Rustup resolves the toolchain from the working directory, and the recipe's
    flags include a nightly-only `-Z` option; outside the repository the same
    command would select whatever stable toolchain the host defaults to and
    fail for a reason that has nothing to do with timeouts.
    """
    _write_lockfile(root)
    completed = subprocess.run(  # noqa: S603 - fixed argv, no shell
        [  # noqa: S607
            "make",
            "-f",
            str(REPO_ROOT / "Makefile"),
            "test-rstest-bdd-ui",
            f"CARGO={shutil.which('cargo')}",
        ],
        cwd=root,
        capture_output=True,
        text=True,
        timeout=900,
        check=False,
        env=_end_to_end_env(),
    )
    return ANSI.sub("", completed.stdout + completed.stderr), completed.returncode


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
    return Run(_resolved_profile(output), outcomes)


def _resolved_profile(output: str) -> str:
    """The profile nextest says it used.

    The banner is the only place nextest states its choice, so a missing banner
    is a broken harness rather than an unproven one. Falling back to the
    requested name would make the profile assertion true by construction - it
    would only restate what the harness asked for, and could not fail when
    nextest ignored it.
    """
    match = PROFILE_LINE.search(output)
    assert match is not None, f"nextest never announced a profile:\n{output[-2000:]}"
    return match["profile"]


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


# The end-to-end contracts run the shipped recipe, so they need a package the
# target names and a workspace the recipe's flags can build. They are kept in
# the repository's `target` directory rather than `tmp_path` for the toolchain
# reason documented on `_make_target`.
END_TO_END_ROOT = REPO_ROOT / "target" / "ui-gate-end-to-end"


def test_the_make_target_supplies_the_lockfile_its_fetch_needs() -> None:
    """A fixture without a lockfile still passes the recipe's `fetch`.

    `test-rstest-bdd-ui` runs after `fetch`, which is `cargo fetch --locked`,
    and `--locked` fails rather than writing a missing lockfile. The fixture is
    generated, so it has none on a clean run; a lockfile left by an earlier
    run would hide that. This contract deletes any lockfile first, so it tests
    the clean case rather than whichever state the directory happened to be in.

    The lockfile is written by Cargo, so this also pins that the version the
    toolchain produces is one `--locked` accepts, rather than one written by
    hand that could drift from the format.

    The exit status is not the signal here. The fixture's test overruns on
    purpose, so the recipe exits non-zero whenever it works; what has to be
    absent is `fetch`'s complaint, and what has to be present is a nextest run
    that got as far as reporting an outcome.
    """
    root = _end_to_end_workspace(END_TO_END_ROOT / "lockfile", with_ceiling=False)
    (root / "Cargo.lock").unlink(missing_ok=True)
    output, _ = _make_target(root)
    assert "cannot create the lock file" not in output, output[-2000:]
    assert _outcomes(output), output[-4000:]


def test_the_make_target_selects_the_profile_and_terminates_per_test() -> None:
    """The shipped recipe, run for real, enforces the named profile's period.

    This is the end-to-end link. `test_rstest_bdd_ui_gate.py` runs the same
    recipe, but under a Cargo stub, so it shows the profile is assigned and not
    that nextest honours it. The contracts above invoke nextest directly, so
    they show the budgets are enforced but not that the shipped recipe selects
    them. Here the recipe itself supplies `NEXTEST_PROFILE` and `-p`, and
    nextest announces the profile and stops the test at that profile's period.

    The default route in this fixture cannot terminate: its period has no
    `terminate-after`, so a run that never selected the named profile would
    report `SLOW` and then the run ceiling's `SIGTERM`.
    """
    root = _end_to_end_workspace(END_TO_END_ROOT / "period", with_ceiling=False)
    output, _ = _make_target(root)
    outcomes = _outcomes(output)
    assert outcomes, output[-4000:]
    assert _resolved_profile(output) == PROFILE, output[-2000:]
    assert _last_kind(outcomes) == "TIMEOUT", _kinds(outcomes)
    assert "SIGTERM" not in _kinds(outcomes), _kinds(outcomes)
    assert _first_time(outcomes, "TIMEOUT") >= END_TO_END_PERIOD - 1, outcomes


def test_the_make_target_enforces_the_profile_ceiling() -> None:
    """The same recipe stops the run at the named profile's `global-timeout`.

    The profile's ceiling is the shortest budget in the file and shorter than
    its own period, so `SLOW` cannot be printed and the `SIGTERM` that arrives
    first can only be the named profile's ceiling. This is the half an override
    cannot supply, and the reason the profile exists at all.
    """
    root = _end_to_end_workspace(END_TO_END_ROOT / "ceiling", with_ceiling=True)
    output, _ = _make_target(root)
    outcomes = _outcomes(output)
    assert outcomes, output[-4000:]
    assert _resolved_profile(output) == PROFILE, output[-2000:]
    assert "SLOW" not in _kinds(outcomes), _kinds(outcomes)
    assert "TIMEOUT" not in _kinds(outcomes), _kinds(outcomes)
    assert _last_kind(outcomes) == "SIGTERM", _kinds(outcomes)
