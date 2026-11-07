"""Run nextest over a fixture workspace, and read back what it reported.

A behavioural contract about time budgets has to say two things about a run:
which profile nextest resolved, and what it did to the test. Both come from
nextest's own output, and neither is available from the exit status - a
per-test kill and a run-wide ceiling both exit 100, and a run that fell back to
the wrong profile still exits with whatever the correct one would have.

This module owns that reading: launching nextest, launching the shipped Make
target, and turning the transcript into :class:`Outcome` values and the profile
nextest announced. It holds no budgets and makes no assertions, so a contract
states what it expects without also restating how to find it.
"""

from __future__ import annotations

import os
import pathlib
import re
import shutil
import subprocess
import tomllib
import typing as typ

import pytest

from ci_workflow_reader import REPO_ROOT
from rstest_bdd_ui_timeout_fixture import TEST_NAME

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

# Both fixture kinds build into one directory beneath the repository's `target`
# so successive contracts reuse the compiled crates rather than rebuilding them.
FIXTURE_TARGET_DIR = "timeout-contract-fixtures"


class Outcome(typ.NamedTuple):
    """One result line, as nextest reported it."""

    kind: str
    seconds: float
    package: str


class Run(typ.NamedTuple):
    """What one nextest invocation reported: the profile it resolved, its results."""

    profile: str
    outcomes: list[Outcome]


def nextest_is_available() -> bool:
    """Whether a real `cargo nextest` can be run in this environment.

    Only the dedicated `test-rstest-bdd-ui-timeouts` target runs these
    contracts, and that target fails before pytest starts when nextest is
    missing. This probe therefore protects a direct `pytest` invocation on a
    machine without a Rust build, not a repository gate: `make
    test-workflow-contracts` excludes the modules, because they are neither fast
    nor Rust-free.

    Only an absent binary or an absent nextest is a skip. A probe that hung, or
    one that Cargo itself failed, is raised: folding those into `False` would
    turn a broken toolchain into a silent skip of every behavioural contract,
    which is exactly the failure this suite exists to catch.
    """
    try:
        probe = subprocess.run(  # noqa: S603 - fixed argv, no shell
            ["cargo", "nextest", "--version"],  # noqa: S607
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
    except OSError as error:
        pytest.skip(f"cargo is not runnable here: {error}")
    except subprocess.TimeoutExpired as error:
        message = "`cargo nextest --version` did not answer within 60 seconds"
        raise RuntimeError(message) from error
    if probe.returncode != 0:
        pytest.skip(f"cargo nextest is unavailable: {probe.stderr.strip()}")
    return True


def end_to_end_env() -> dict[str, str]:
    """The environment the real Make target is run in.

    Unlike `fixture_env`, the target's own flags are left alone: the end-to-end
    contracts exist to run what production runs, and the recipe's `RUSTFLAGS`
    and `NEXTEST_PROFILE` are part of that. Only the environment that would move
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
    env["CARGO_TARGET_DIR"] = str(REPO_ROOT / "target" / FIXTURE_TARGET_DIR)
    return env


def fixture_env(profile: str | None = None) -> dict[str, str]:
    """The environment the fixture is built and run in.

    The lane's build flags are removed rather than inherited. They are tuned for
    this workspace - CI exports nightly-only `-Z` flags, and `make lint` carries
    `-D warnings` - and the fixture is a plain leaf crate that must build under
    whatever toolchain the lane has. Inheriting them made `cargo metadata` fail
    before nextest ever ran, which is a harness failure, not a timeout result.

    `NEXTEST_PROFILE` is set rather than inherited, for the same reason: the
    contracts are decided by which profile governs, so an ambient value left
    over from a surrounding run must not choose it. `None` selects `default`
    explicitly, which keeps the default-route contracts running the route they
    describe.

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
    env["CARGO_TARGET_DIR"] = str(REPO_ROOT / "target" / FIXTURE_TARGET_DIR)
    env["NEXTEST_PROFILE"] = profile or "default"
    return env


def write_lockfile(root: pathlib.Path) -> None:
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
        env=end_to_end_env(),
    )
    if completed.returncode != 0:
        message = f"could not write the fixture lockfile:\n{completed.stderr}"
        raise RuntimeError(message)


def make_target(root: pathlib.Path) -> tuple[str, int]:
    """Run the real `test-rstest-bdd-ui` recipe in `root`, and report its output.

    `make -f <repo>/Makefile` with a fixture working directory is what makes
    this end-to-end: the recipe that runs is the shipped one, flags and profile
    assignment included, and only the workspace it compiles belongs to the
    fixture. `CARGO` is pinned to the real binary because the Makefile prepends
    `$(HOME)/.cargo/bin` to `PATH`, which would otherwise shadow any stub.

    The fixture lives under the repository's `target` directory on purpose.
    Rustup resolves the toolchain from the working directory, and the recipe's
    flags include a nightly-only `-Z` option; outside the repository the same
    command would select whatever stable toolchain the host defaults to and
    fail for a reason that has nothing to do with timeouts.
    """
    write_lockfile(root)
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
        env=end_to_end_env(),
    )
    return ANSI.sub("", completed.stdout + completed.stderr), completed.returncode


def run(root: pathlib.Path, package: str, profile: str | None = None) -> Run:
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
        env=fixture_env(profile),
    )
    output = ANSI.sub("", completed.stdout + completed.stderr)
    outcomes = read_outcomes(output)
    if not outcomes:
        raise AssertionError(
            f"nextest reported no result for {package} in {root} "
            f"under profile {profile or 'default'} "
            f"(exit {completed.returncode}):\n{output[-4000:]}"
        )
    return Run(resolved_profile(output), outcomes)


def resolved_profile(output: str) -> str:
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


def read_outcomes(output: str) -> list[Outcome]:
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


def kinds(outcomes: list[Outcome]) -> list[str]:
    """The result kinds reported for one fixture member."""
    return [outcome.kind for outcome in outcomes]


def last_kind(outcomes: list[Outcome]) -> str:
    """The final result nextest reported for the fixture member."""
    assert outcomes, "nextest reported no result for the fixture member"
    return outcomes[-1].kind


def first_time(outcomes: list[Outcome], kind: str) -> float:
    """When nextest reported `kind`, as the contract's ordering evidence."""
    matching = [outcome.seconds for outcome in outcomes if outcome.kind == kind]
    assert matching, f"nextest never reported {kind}: {kinds(outcomes)}"
    return matching[0]


def production_config() -> dict[str, typ.Any]:
    """This repository's own nextest configuration, parsed."""
    return tomllib.loads(
        (REPO_ROOT / ".config" / "nextest.toml").read_text(encoding="utf-8")
    )


def production_overrides() -> list[dict[str, typ.Any]]:
    """Every package override in the repository's own nextest configuration."""
    overrides: list[dict[str, typ.Any]] = (
        production_config()["profile"]["default"].get("overrides") or []
    )
    return overrides


def production_profile_name() -> str:
    """The profile name `make test-rstest-bdd-ui` passes to nextest.

    Read out of the recipe rather than restated, so a contract that selects the
    fixture's profile by name is evidence about the target only while the two
    agree. A rename on either side makes this the failing assertion.
    """
    makefile = (REPO_ROOT / "Makefile").read_text(encoding="utf-8")
    target = makefile.split("\ntest-rstest-bdd-ui:", 1)[1].split("\n\n", 1)[0]
    prefix = "NEXTEST_PROFILE="
    assigned = [word for word in target.split() if word.startswith(prefix)]
    assert assigned, f"the UI target assigns no profile:\n{target}"
    return assigned[0][len(prefix) :]
