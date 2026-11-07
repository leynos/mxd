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
deliberately does not, so the same run exercises a per-package timeout and the
default route's fall-through to the run ceiling. Nothing here touches the real
UI package or the shared compile-admission queue: the fixture has no
dependencies, so its build is a single leaf crate per member.
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

# The fixture's budgets, in seconds. They are short enough to keep the lane
# cheap and far enough apart that ordering, not a tight window, is what the
# contracts assert: the override must fire before the run ceiling, and the
# control must reach the ceiling without ever being terminated per-test.
OVERRIDE_PERIOD = 3
DEFAULT_PERIOD = 2
GLOBAL_TIMEOUT = 6
TEST_SLEEP = 60

ANSI = re.compile(r"\x1b\[[0-9;]*m")
RESULT_LINE = re.compile(
    r"^\s*(?P<kind>TIMEOUT|SIGTERM|SLOW|PASS|FAIL)"
    r".*?\[[>\s]*(?P<seconds>\d+(?:\.\d+)?)s\]"
    r".*?(?P<package>crate-[a-z]+)\s+(?P<test>tests::\w+)"
)


@dataclasses.dataclass(frozen=True)
class Outcome:
    """One result line, as nextest reported it."""

    kind: str
    seconds: float
    package: str


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


def _config(terminate: bool) -> str:
    """The fixture's nextest configuration.

    `terminate` decides whether the overridden package's period kills the test.
    Turning it off is how the "slow warning" outcome is produced without
    changing anything else.
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
    )


def _workspace(root: pathlib.Path, terminate: bool) -> pathlib.Path:
    """Create the two-member fixture workspace and return its path."""
    root.mkdir(parents=True, exist_ok=True)
    (root / "Cargo.toml").write_text(
        f'[workspace]\nresolver = "3"\nmembers = ["{OVERRIDDEN}", "{CONTROL}"]\n',
        encoding="utf-8",
    )
    _member(root / OVERRIDDEN, OVERRIDDEN)
    _member(root / CONTROL, CONTROL)
    (root / ".config").mkdir()
    (root / ".config" / "nextest.toml").write_text(_config(terminate), encoding="utf-8")
    return root


def _fixture_env() -> dict[str, str]:
    """The environment the fixture is built in.

    The lane's build flags are removed rather than inherited. They are tuned for
    this workspace - CI exports nightly-only `-Z` flags, and `make lint` carries
    `-D warnings` - and the fixture is a plain leaf crate that must build under
    whatever toolchain the lane has. Inheriting them made `cargo metadata` fail
    before nextest ever ran, which is a harness failure, not a timeout result.

    `RUSTC_WRAPPER` is deliberately kept: compile admission is not bypassed.
    """
    env = {
        key: value
        for key, value in os.environ.items()
        if key not in {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS"}
    }
    env["CARGO_TARGET_DIR"] = str(REPO_ROOT / "target" / "timeout-contract-fixtures")
    return env


def _run(root: pathlib.Path, package: str) -> list[Outcome]:
    """Run one fixture member under the fixture's nextest configuration.

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
            "-E",
            f"test({TEST_NAME}) and package({package})",
        ],
        cwd=root,
        capture_output=True,
        text=True,
        timeout=600,
        check=False,
        env=_fixture_env(),
    )
    output = completed.stdout + completed.stderr
    outcomes = _outcomes(output)
    if not outcomes:
        raise AssertionError(
            f"nextest reported no result for {package} in {root} "
            f"(exit {completed.returncode}):\n{ANSI.sub('', output)[-4000:]}"
        )
    return outcomes


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
    outcomes = _run(_workspace(tmp_path / "terminating", terminate=True), OVERRIDDEN)
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
    outcomes = _run(_workspace(tmp_path / "warning", terminate=False), OVERRIDDEN)
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
    outcomes = _run(_workspace(tmp_path / "control", terminate=True), CONTROL)
    assert "TIMEOUT" not in _kinds(outcomes), _kinds(outcomes)
    assert _last_kind(outcomes) == "SIGTERM", _kinds(outcomes)


def test_the_override_fires_before_the_run_ceiling(tmp_path: pathlib.Path) -> None:
    """The two budgets act in the order their values imply.

    Ordering rather than a fixed window is what is asserted: a loaded host can
    delay either report, but it cannot make the shorter budget act later than
    the longer one.
    """
    root = _workspace(tmp_path / "ordering", terminate=True)
    overridden = _first_time(_run(root, OVERRIDDEN), "TIMEOUT")
    control = _first_time(_run(root, CONTROL), "SIGTERM")
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
