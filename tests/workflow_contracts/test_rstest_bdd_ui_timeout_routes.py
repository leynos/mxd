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
outcome is attributed to the crate that produced it.

Both routes terminate, because both routes in production terminate: the default
profile kills a test at 60 seconds and the override raises that to 900. Scaling
the *shape* rather than only the numbers is what makes the fixture evidence.
The override buys the matching crate a longer budget, so one test sleep sits
between the two periods: the matching crate finishes it and the control is
killed partway through. A fixture whose default route merely warned could not
express that, and would leave the fall-through path untested.

This module covers the two routes into the package - the override and the
default fall-through - and checks the fixture against production's shape. The
profile half lives in `test_rstest_bdd_ui_timeout_profile.py`, because a
contract file that grew to cover both would be the one file in this suite too
long to read straight through. Both are run by
`make test-rstest-bdd-ui-timeouts`, and only by that target: `make
test-workflow-contracts` excludes the pair by glob, because the fixtures take
minutes and the docs-tooling lane it runs in has no nextest or Rust build.
`test_rstest_bdd_ui_gate.py` pins both halves of that wiring, since a lane that
quietly stopped running these files would look green.
"""

from __future__ import annotations

import pathlib

import pytest

from rstest_bdd_ui_timeout_fixture import (
    CONTROL,
    DEFAULT_PERIOD,
    GLOBAL_TIMEOUT,
    OVERRIDDEN,
    OVERRIDE_PERIOD,
    SURVIVOR_SLEEP,
    Shape,
    config,
    workspace,
)
from rstest_bdd_ui_timeout_runner import (
    first_time,
    fixture_env,
    kinds,
    last_kind,
    nextest_is_available,
    production_config,
    production_overrides,
    run,
)

pytestmark = pytest.mark.skipif(
    not nextest_is_available(),
    reason="a real cargo nextest is required for the behavioural contracts",
)


def test_an_overrunning_test_is_terminated_by_the_package_override(
    tmp_path: pathlib.Path,
) -> None:
    """The override kills the test it matches, at its own period.

    `TIMEOUT` is nextest's word for a per-test termination, so this fails if
    `terminate-after` is dropped from the override: the same test then runs on
    and the run reports `SIGTERM` at the ceiling instead.
    """
    outcomes = run(workspace(tmp_path / "terminating"), OVERRIDDEN).outcomes
    assert last_kind(outcomes) == "TIMEOUT", kinds(outcomes)
    assert "SIGTERM" not in kinds(outcomes), kinds(outcomes)
    assert first_time(outcomes, "TIMEOUT") >= OVERRIDE_PERIOD - 1, outcomes


def test_the_override_lets_a_test_survive_a_budget_that_would_kill_it(
    tmp_path: pathlib.Path,
) -> None:
    """The override is time the test gains, not a relabelled outcome.

    Both crates run the same test for the same short sleep, and both routes
    terminate. The only difference between them is the package name the filter
    matches, so the difference in result is the override itself: the control is
    held to the default period and killed, and the matching crate runs past
    that point and finishes.

    This is what a fixture whose default route only warned could not show. It
    is also the shape production has - the default profile kills at 60 seconds
    and the UI package's override raises that to 900 - so a run that ignored
    the override would fail here the way it would in CI.
    """
    root = workspace(tmp_path / "survivor", Shape(sleep_seconds=SURVIVOR_SLEEP))
    overridden = run(root, OVERRIDDEN)
    control = run(root, CONTROL)
    assert last_kind(overridden.outcomes) == "PASS", kinds(overridden.outcomes)
    assert "TIMEOUT" not in kinds(overridden.outcomes), kinds(overridden.outcomes)
    assert last_kind(control.outcomes) == "TIMEOUT", kinds(control.outcomes)
    assert first_time(control.outcomes, "TIMEOUT") < SURVIVOR_SLEEP, control.outcomes


def test_the_termination_this_repository_configures_is_load_bearing(
    tmp_path: pathlib.Path,
) -> None:
    """Dropping this repository's `terminate-after` changes the outcome.

    The claim under test is about `.config/nextest.toml`, not about nextest: the
    `terminate-after` this repository sets on every route is what turns an
    overrun into a killed test. So the contract first asserts the setting is
    there - otherwise there would be nothing to remove - then seeds exactly one
    mutation, deleting it from the fixture's default route, and runs the same
    crate against both the real shape and the mutated one.

    Removing it takes `TIMEOUT` away: the period still expires, but it now only
    reports `SLOW` and the run ends at the ceiling instead. That difference is
    the setting's contribution, and it is why a route without the setting would
    pass a budget assertion while leaving an overrunning test running.

    Only `TIMEOUT` is asserted to be absent, not how the run then ends. What
    the run does after a warning period is nextest's business; what belongs to
    this repository is whether its configuration stops the test at all.
    """
    production = production_config()
    default = production["profile"]["default"]["slow-timeout"]
    overrides = production["profile"]["default"].get("overrides") or []
    assert default["terminate-after"] == 1, default
    assert all(o["slow-timeout"]["terminate-after"] == 1 for o in overrides), overrides

    mutated = run(workspace(tmp_path / "warning", Shape(terminate=False)), CONTROL)
    assert "TIMEOUT" not in kinds(mutated.outcomes), kinds(mutated.outcomes)
    assert "SLOW" in kinds(mutated.outcomes), kinds(mutated.outcomes)

    unmutated = run(workspace(tmp_path / "unmutated"), CONTROL).outcomes
    assert last_kind(unmutated) == "TIMEOUT", kinds(unmutated)


def test_a_non_matching_package_is_held_to_the_default_route(
    tmp_path: pathlib.Path,
) -> None:
    """An override does not leak to a package it does not match.

    The control crate matches no override, so the default route's period is
    the one that kills it. A filter that matched too broadly would give it the
    override's longer budget, and it would be killed later instead.
    """
    outcomes = run(workspace(tmp_path / "control"), CONTROL).outcomes
    assert last_kind(outcomes) == "TIMEOUT", kinds(outcomes)
    assert first_time(outcomes, "TIMEOUT") >= DEFAULT_PERIOD - 1, outcomes
    assert first_time(outcomes, "TIMEOUT") < OVERRIDE_PERIOD, outcomes


def test_the_override_extends_the_kill_point_without_reaching_the_ceiling(
    tmp_path: pathlib.Path,
) -> None:
    """The two budgets act in the order their values imply.

    Ordering rather than a fixed window is what is asserted: a loaded host can
    delay either report, but it cannot reorder two budgets. The ceiling sits
    above both, so the later of the two kills is still a period rather than the
    run-wide stop, which is what keeps this a comparison of the periods.
    """
    root = workspace(tmp_path / "ordering")
    control = first_time(run(root, CONTROL).outcomes, "TIMEOUT")
    overridden = first_time(run(root, OVERRIDDEN).outcomes, "TIMEOUT")
    assert control < overridden, (control, overridden)
    assert overridden < GLOBAL_TIMEOUT, (overridden, GLOBAL_TIMEOUT)


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
    env = fixture_env()
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
    production_filters = [override["filter"] for override in production_overrides()]
    assert all("package(" in f for f in production_filters), production_filters
    assert f'filter = "package({OVERRIDDEN})"' in config(terminate=True)
