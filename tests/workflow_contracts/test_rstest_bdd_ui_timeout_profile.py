"""Real-nextest proof that the `rstest-bdd-ui` profile is selected and enforced.

`global-timeout` is not one of the settings a per-package override can replace,
so the run-wide headroom lives in the `rstest-bdd-ui` profile, which
`make test-rstest-bdd-ui` selects through `NEXTEST_PROFILE`. Reading the file
proves the budgets are written down. Only a run proves nextest compiles that
profile, selects it, and enforces both halves of it.

Two halves, and they are different halves. The profile's period is longer than
the default's, so a run that fell back to `default` would kill the same test
earlier - the elapsed time, not the word, is what separates them. And the
profile's ceiling governs the run as a whole, which is the part an override
cannot reach at all.

A third group runs the shipped `test-rstest-bdd-ui` recipe itself against a
fixture, so what is exercised is the target production runs rather than an
equivalent command written here. Nothing here touches the real UI package or
the shared compile-admission queue: the fixtures have no dependencies, so each
build is a single leaf crate.

The fixture itself, and the contracts covering the two routes into the package,
live in `rstest_bdd_ui_timeout_fixture.py` and
`test_rstest_bdd_ui_timeout_routes.py`. Both modules are run by
`make test-rstest-bdd-ui-timeouts`, and only by that target.
"""

from __future__ import annotations

import pathlib
import shutil
import tempfile
import typing as typ

import pytest

from ci_workflow_reader import REPO_ROOT
from rstest_bdd_ui_timeout_fixture import (
    CONTROL,
    END_TO_END_PERIOD,
    GLOBAL_TIMEOUT,
    PROFILE,
    PROFILE_PERIOD,
    TIGHT_PROFILE_GLOBAL,
    TIGHT_PROFILE_PERIOD,
    Shape,
    config,
    end_to_end_workspace,
    workspace,
)
from rstest_bdd_ui_timeout_runner import (
    first_time,
    kinds,
    last_kind,
    make_target,
    nextest_is_available,
    production_config,
    production_profile_name,
    read_outcomes,
    resolved_profile,
    run,
)

pytestmark = pytest.mark.skipif(
    not nextest_is_available(),
    reason="a real cargo nextest is required for the behavioural contracts",
)


def test_the_profile_name_is_the_one_the_make_target_selects() -> None:
    """The fixture's profile name is read out of the target, not guessed.

    The named-profile contracts below are only evidence about `make
    test-rstest-bdd-ui` if they select the same profile that target selects. A
    rename on either side would otherwise leave them passing against a profile
    the Makefile no longer uses.
    """
    assert production_profile_name() == PROFILE
    assert PROFILE in production_config()["profile"]
    assert f"[profile.{PROFILE}]" in config(terminate=True)


def test_the_named_profile_selects_and_enforces_its_own_period(
    tmp_path: pathlib.Path,
) -> None:
    """A run under the named profile reports `TIMEOUT` at that profile's period.

    Two things are asserted, and they are different things: nextest announced
    the profile by name, and the kill landed at that profile's period rather
    than the default's. The fixture's named period is longer than the default
    one, and both routes terminate, so a run that fell back to `default` would
    report the same word at a smaller number.

    This is the link the file-reading contracts cannot supply. They prove the
    budgets are written; this proves nextest compiles the profile, selects it,
    and acts on it.
    """
    named = run(workspace(tmp_path / "named"), CONTROL, profile=PROFILE)
    assert named.profile == PROFILE, named.profile
    assert last_kind(named.outcomes) == "TIMEOUT", kinds(named.outcomes)
    assert first_time(named.outcomes, "TIMEOUT") >= PROFILE_PERIOD - 1, named.outcomes


def test_the_named_profile_ceiling_stops_the_run_when_it_comes_first(
    tmp_path: pathlib.Path,
) -> None:
    """The named profile's own `global-timeout` governs its run.

    With a ceiling shorter than its period, the period cannot fire first, so
    `TIMEOUT` is unreachable rather than merely unlikely and its absence is
    strong evidence. It is also what rules out the default route: its period is
    far shorter than this ceiling, so a run that fell back to `default` would
    have terminated the test per-test and reported `TIMEOUT` instead.
    """
    shape = Shape(
        profile_period=TIGHT_PROFILE_PERIOD,
        profile_global=TIGHT_PROFILE_GLOBAL,
    )
    named = run(workspace(tmp_path / "named-ceiling", shape), CONTROL, profile=PROFILE)
    assert named.profile == PROFILE, named.profile
    assert "SLOW" not in kinds(named.outcomes), kinds(named.outcomes)
    assert "TIMEOUT" not in kinds(named.outcomes), kinds(named.outcomes)
    assert last_kind(named.outcomes) == "SIGTERM", kinds(named.outcomes)
    assert first_time(named.outcomes, "SIGTERM") < GLOBAL_TIMEOUT, named.outcomes


def test_the_default_route_is_what_the_fixture_selects_when_asked(
    tmp_path: pathlib.Path,
) -> None:
    """The unnamed route really is `default`, so the comparison above is sound.

    The named-profile contract above argues from a difference: the same crate
    under `default` is killed earlier. That argument needs the difference to
    exist, so this pins the other half - `default` is selected, and the kill
    lands at its shorter period rather than the profile's.
    """
    unnamed = run(workspace(tmp_path / "unnamed"), CONTROL)
    assert unnamed.profile == "default", unnamed.profile
    assert last_kind(unnamed.outcomes) == "TIMEOUT", kinds(unnamed.outcomes)
    assert first_time(unnamed.outcomes, "TIMEOUT") < PROFILE_PERIOD, unnamed.outcomes


@pytest.fixture
def end_to_end_root() -> typ.Iterator[pathlib.Path]:
    """A scratch workspace directory inside the repository, removed afterwards.

    The end-to-end contracts run the shipped recipe, so they need a package the
    target names and a workspace the recipe's flags can build. Each gets its
    own directory beneath the repository's `target`: the location is inside the
    repository for the toolchain reason documented on `make_target`, and being
    per-test is what stops one contract from reading sources or a lockfile
    another one left behind.
    """
    parent = REPO_ROOT / "target"
    parent.mkdir(exist_ok=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix="ui-gate-end-to-end-", dir=parent))
    try:
        yield root
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_the_harness_supplies_the_lockfile_the_recipe_fetch_needs(
    end_to_end_root: pathlib.Path,
) -> None:
    """The fixture's lockfile is written by Cargo, not by hand.

    `test-rstest-bdd-ui` runs after `fetch`, which is `cargo fetch --locked`,
    and `--locked` refuses to write a missing lockfile. The fixture is generated
    rather than checked in, so something has to supply one before the recipe
    starts; `make_target` does, by running `cargo generate-lockfile`. That is
    the thing this contract names, so it asserts what the harness produces
    rather than what the recipe complains about.

    Because Cargo writes the file, this also pins that the version the installed
    toolchain emits is one `--locked` accepts, rather than one written here that
    could drift from the format.

    The exit status is not the signal. The fixture's test overruns on purpose,
    so the recipe exits non-zero whenever it works; what has to be present is a
    nextest run that got as far as reporting an outcome.
    """
    root = end_to_end_workspace(end_to_end_root / "lockfile", with_ceiling=False)
    (root / "Cargo.lock").unlink(missing_ok=True)
    assert not (root / "Cargo.lock").exists(), "the contract needs a clean start"
    output, _ = make_target(root)
    assert (root / "Cargo.lock").is_file(), output[-2000:]
    assert read_outcomes(output), output[-4000:]


def test_the_make_target_selects_the_profile_and_terminates_per_test(
    end_to_end_root: pathlib.Path,
) -> None:
    """The shipped recipe, run for real, enforces the named profile's period.

    This is the end-to-end link. `test_rstest_bdd_ui_gate.py` runs the same
    recipe, but under a Cargo stub, so it shows the profile is assigned and not
    that nextest honours it. The contracts above invoke nextest directly, so
    they show the budgets are enforced but not that the shipped recipe selects
    them. Here the recipe itself supplies `NEXTEST_PROFILE` and `-p`, and
    nextest announces the profile and stops the test at that profile's period.

    The default route in this fixture terminates too, as production's does, and
    at a shorter period. A run that never selected the named profile would
    therefore also report `TIMEOUT` - just earlier. The elapsed time is what
    separates the two, so the kill point, not the word, is the assertion.
    """
    root = end_to_end_workspace(end_to_end_root / "period", with_ceiling=False)
    output, _ = make_target(root)
    outcomes = read_outcomes(output)
    assert outcomes, output[-4000:]
    assert resolved_profile(output) == PROFILE, output[-2000:]
    assert last_kind(outcomes) == "TIMEOUT", kinds(outcomes)
    assert "SIGTERM" not in kinds(outcomes), kinds(outcomes)
    assert first_time(outcomes, "TIMEOUT") >= END_TO_END_PERIOD - 1, outcomes


def test_the_make_target_enforces_the_profile_ceiling(
    end_to_end_root: pathlib.Path,
) -> None:
    """The same recipe stops the run at the named profile's `global-timeout`.

    The profile's ceiling is the shortest budget in the file and shorter than
    its own period, so `SLOW` cannot be printed and the `SIGTERM` that arrives
    first can only be the named profile's ceiling. This is the half an override
    cannot supply, and the reason the profile exists at all.
    """
    root = end_to_end_workspace(end_to_end_root / "ceiling", with_ceiling=True)
    output, _ = make_target(root)
    outcomes = read_outcomes(output)
    assert outcomes, output[-4000:]
    assert resolved_profile(output) == PROFILE, output[-2000:]
    assert "SLOW" not in kinds(outcomes), kinds(outcomes)
    assert "TIMEOUT" not in kinds(outcomes), kinds(outcomes)
    assert last_kind(outcomes) == "SIGTERM", kinds(outcomes)
