"""The disposable nextest workspaces the behavioural timeout contracts run.

The contracts exist to show that nextest acts on the budgets this repository
configures, rather than merely that the numbers are written down. Showing that
needs a workspace nextest can actually run, with budgets small enough to spend
seconds rather than minutes. That is what this module builds: two-member leaf
crates, a `.config/nextest.toml` scaled to the shape production has, and the
same again for the package the shipped Make target names.

The budgets live here rather than in a contract because they are not
independent. A sleep only means something relative to the periods that might
kill it, and a profile's ceiling only means something relative to the period it
cuts short. One module states those relationships once, and every contract
reads the shape it needs from here instead of restating numbers it would then
have to keep in step.

:mod:`rstest_bdd_ui_timeout_runner` runs nextest over these workspaces and reads
back what it reported. See ``docs/developers-guide.md``, "Compile-time scenario
contracts", for the design.
"""

from __future__ import annotations

import dataclasses
import pathlib

OVERRIDDEN = "crate-overridden"
CONTROL = "crate-control"
TEST_NAME = "overruns"

# The profile `make test-rstest-bdd-ui` selects. `_production_profile_name`
# reads the same name back out of the Makefile, so this literal cannot drift
# from the target it stands for without a contract failing.
PROFILE = "rstest-bdd-ui"

# The fixture's budgets, in seconds. They are short enough to keep the lane
# cheap and far enough apart that ordering, not a tight window, is what the
# contracts assert. Each pair of numbers below is separated by at least two
# seconds where it matters, because a loaded host can delay a report by a
# little but cannot reorder two budgets.
#
# Both routes terminate, because both routes in production are terminating: the
# default profile kills at 60 seconds and the package override raises that to
# 900. The fixture scales that shape rather than only the numbers. Because both
# routes report `TIMEOUT`, the outcome word cannot tell them apart here, and the
# contracts read the second at which the kill landed instead. That is the whole
# benefit the override buys - the same test survives longer - and it is the
# property a fixture whose default route merely warned could not express.
OVERRIDE_PERIOD = 6
DEFAULT_PERIOD = 2
# The ceiling sits well above both periods, so neither route reaches it before
# its own period fires. A contract comparing the two kill points is therefore
# measuring the periods rather than whichever budget happened to be shortest.
# It stays below `TEST_SLEEP` so a route that never terminates still ends at
# the ceiling rather than outliving the test.
GLOBAL_TIMEOUT = 30
TEST_SLEEP = 60

# A sleep that fits between the two periods: long enough for the default route
# to kill it, short enough for the override to let it finish. Running the same
# crate under both routes is what shows the override changes the outcome rather
# than only the elapsed time.
SURVIVOR_SLEEP = 4

# The named profile as the fixture normally declares it. Its period is longer
# than `DEFAULT_PERIOD`, so a run that fell back to `default` would be killed at
# 2 seconds where this one survives to 5. The elapsed time is what discriminates
# here, since both routes report the same word.
PROFILE_PERIOD = 5
PROFILE_GLOBAL = 20

# A second shape, where the profile's ceiling is shorter than its period and
# shorter than every period in the file, the default route's included. Neither
# period can fire before the ceiling, so `TIMEOUT` cannot be printed at all -
# its absence is structural rather than a race - and the only reachable outcome
# is `SIGTERM` at the profile's ceiling. It is also what rules out the default
# route, which would otherwise terminate the test per-test at its own period.
TIGHT_PROFILE_PERIOD = 30
TIGHT_PROFILE_GLOBAL = 1

# The end-to-end fixture drives the real Make target, which hardcodes this
# package name. Its budgets scale the same shape as above: both routes
# terminate, the named profile's period is the longer of the two, and its
# ceiling is above both. A run that failed to select the named profile is
# therefore stopped early by the default route, which is a bounded failure
# rather than a hang.
UI_PACKAGE = "rstest-bdd-ui-tests"
END_TO_END_PERIOD = 6
END_TO_END_GLOBAL = 40
# The default route's period in that fixture, kept well below the profile's so
# the two kill points cannot be confused under load.
END_TO_END_DEFAULT_PERIOD = 2
# The default route's ceiling in the end-to-end fixture, high enough that it is
# never the budget that stops the run in either shape.
END_TO_END_DEFAULT_GLOBAL = 30


@dataclasses.dataclass(frozen=True)
class Shape:
    """One fixture shape: every knob a contract may want to vary, in one value.

    These four settings are not independent. Raising the sleep only means
    something relative to the periods that might kill it, and shortening a
    profile's ceiling only means something relative to the period it cuts
    short. Passing them as one value keeps a call site saying *which shape*
    rather than restating the relationship in positional arguments, where the
    second and third numbers could be transposed and still look plausible.

    The defaults are the shape most contracts want: both routes terminating, the
    override's period the longer of the two, and a sleep that outlasts every
    budget so termination is always observable.
    """

    terminate: bool = True
    profile_period: int = PROFILE_PERIOD
    profile_global: int = PROFILE_GLOBAL
    sleep_seconds: int = TEST_SLEEP


def member(directory: pathlib.Path, name: str, sleep_seconds: int = TEST_SLEEP) -> None:
    """Write one fixture member whose test sleeps for `sleep_seconds`.

    The default sleep overruns every budget, which is what the contracts about
    termination need. A shorter one fits under the override's period while
    still overrunning the default's, so the same crate can be shown to fail
    under one route and pass under the other.
    """
    (directory / "src").mkdir(parents=True, exist_ok=True)
    (directory / "Cargo.toml").write_text(
        f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2024"\n',
        encoding="utf-8",
    )
    (directory / "src" / "lib.rs").write_text(
        f"//! Fixture member for the timeout contracts.\n\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    //! The one test nextest is asked to run.\n"
        "    //!\n"
        "    //! It sleeps rather than asserts, because the contracts are about\n"
        "    //! what nextest does to a test that overruns a budget, not about\n"
        "    //! what the test computes. The sleep is set per fixture so one\n"
        "    //! crate can be run under both routes and outlast only one.\n"
        f"    /// Sleeps for {sleep_seconds} seconds.\n"
        "    #[test]\n"
        f"    fn {TEST_NAME}() {{\n"
        f"        std::thread::sleep(std::time::Duration::from_secs({sleep_seconds}));\n"
        "    }\n"
        "}\n",
        encoding="utf-8",
    )


def config(
    terminate: bool,
    profile_period: int = PROFILE_PERIOD,
    profile_global: int = PROFILE_GLOBAL,
) -> str:
    """The fixture's nextest configuration.

    `terminate` decides whether the *default* route's period kills the test.
    The override always does, because production's does; `False` is the shape
    that isolates what the override contributes, by giving the control route a
    period that only warns. Comparing a matching run against a warning control
    is how a contract shows the kill came from the override rather than from
    the default budget underneath it.

    The `rstest-bdd-ui` profile is declared here with second-scale budgets, in
    the same shape production declares it, so the behavioural contracts can
    select the profile *name* production selects. A profile that is absent, or
    whose tables do not parse, is an error nextest reports at startup; a profile
    that parses but is never selected silently falls back to `default`. Only a
    run covers both.
    """
    default_termination = ", terminate-after = 1" if terminate else ""
    return (
        "[profile.default]\n"
        f'slow-timeout = {{ period = "{DEFAULT_PERIOD}s"'
        f'{default_termination}, grace-period = "1s" }}\n'
        f'global-timeout = "{GLOBAL_TIMEOUT}s"\n'
        "\n"
        "[[profile.default.overrides]]\n"
        f'filter = "package({OVERRIDDEN})"\n'
        f'slow-timeout = {{ period = "{OVERRIDE_PERIOD}s", '
        'terminate-after = 1, grace-period = "1s" }\n'
        "\n"
        f"[profile.{PROFILE}]\n"
        f'slow-timeout = {{ period = "{profile_period}s", '
        'terminate-after = 1, grace-period = "1s" }\n'
        f'global-timeout = "{profile_global}s"\n'
    )


def workspace(root: pathlib.Path, shape: Shape = Shape()) -> pathlib.Path:
    """Create the two-member fixture workspace and return its path.

    `shape.sleep_seconds` sets both members' tests to the same duration.
    Shortening it to `SURVIVOR_SLEEP` lets a contract hold one identical test to
    both routes at once: the two crates differ in nothing but their name, which
    is all the package filter keys on, so any difference in outcome is the
    route.
    """
    root.mkdir(parents=True, exist_ok=True)
    (root / "Cargo.toml").write_text(
        f'[workspace]\nresolver = "3"\nmembers = ["{OVERRIDDEN}", "{CONTROL}"]\n',
        encoding="utf-8",
    )
    member(root / OVERRIDDEN, OVERRIDDEN, shape.sleep_seconds)
    member(root / CONTROL, CONTROL, shape.sleep_seconds)
    (root / ".config").mkdir()
    (root / ".config" / "nextest.toml").write_text(
        config(shape.terminate, shape.profile_period, shape.profile_global),
        encoding="utf-8",
    )
    return root


def end_to_end_workspace(root: pathlib.Path, with_ceiling: bool) -> pathlib.Path:
    """Create a fixture workspace holding the package the Make target names.

    The target hardcodes `-p rstest-bdd-ui-tests`, so the fixture has to supply
    a package by that name or the target would fail before nextest ran. The
    fixture is deliberately not the real package: it is one leaf crate with no
    dependencies, so the end-to-end contract costs a single compile rather than
    a build of the UI gate's trybuild harness.

    `with_ceiling` chooses which half under test. With it, the profile's
    ceiling is the shortest budget in the file, so reaching it can only be the
    named profile governing. Without it, the ceiling is long enough that the
    profile's period fires first, and its own `terminate-after` is what stops
    the test.

    The default route terminates here as it does in production, and its period
    is the shortest budget in both shapes. A run that failed to select the
    profile would therefore still be killed, just at the wrong, earlier point -
    so the elapsed time, not the outcome word, is what the contracts read.

    The same directory is reused across runs, so every write is idempotent: a
    second run must reproduce the fixture rather than fail on a leftover one.
    """
    root.mkdir(parents=True, exist_ok=True)
    (root / "Cargo.toml").write_text(
        f'[workspace]\nresolver = "3"\nmembers = ["{UI_PACKAGE}"]\n',
        encoding="utf-8",
    )
    member(root / UI_PACKAGE, UI_PACKAGE)
    (root / ".config").mkdir(exist_ok=True)
    if with_ceiling:
        profile_period, profile_ceiling = TIGHT_PROFILE_PERIOD, TIGHT_PROFILE_GLOBAL
    else:
        profile_period, profile_ceiling = END_TO_END_PERIOD, END_TO_END_GLOBAL
    (root / ".config" / "nextest.toml").write_text(
        "[profile.default]\n"
        f'slow-timeout = {{ period = "{END_TO_END_DEFAULT_PERIOD}s", '
        'terminate-after = 1, grace-period = "1s" }\n'
        f'global-timeout = "{END_TO_END_DEFAULT_GLOBAL}s"\n'
        "\n"
        f"[profile.{PROFILE}]\n"
        f'slow-timeout = {{ period = "{profile_period}s", '
        'terminate-after = 1, grace-period = "1s" }\n'
        f'global-timeout = "{profile_ceiling}s"\n',
        encoding="utf-8",
    )
    return root
