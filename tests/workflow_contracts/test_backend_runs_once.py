"""Each database backend has a test run in CI, and the SQLite suite runs once.

The build-test sqlite leg once ran the same suite as the coverage job's SQLite
step (`--features sqlite,test-support`, default features on), so every event
paid for it twice. Deduplication is a hazard as well as a saving: removing the
wrong copy leaves a backend untested while every check stays green. The rule
these contracts hold is that both backends are first-class, so neither may lose
its only run, and the SQLite suite must not run again in a second job.

The judgement is a query over a parsed workflow, :func:`ci_backend_runs.suite_runs`, that lists
every suite the workflow runs as a feature set. The repository's `ci.yml` is
read through it, and so are mutated copies, so the query is driven against the
defects it exists to catch rather than only matched against a healthy file.
"""

from __future__ import annotations

import collections
import copy
import typing as typ

import pytest
from ci_backend_runs import (
    LINT_FLAGS,
    NEXTEST_STEP,
    POSTGRES_SUITE,
    SQLITE_SUITE,
    leg_runs_step,
    lint_problems,
    suite_runs,
)
from ci_workflow_reader import repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

    from ci_backend_runs import Document


@pytest.fixture(scope="module")
def ci() -> Document:
    """This repository's `ci.yml`."""
    return repository_documents()["ci.yml"]


def _sqlite_runs(document: Document) -> list[str]:
    """Where the default-feature SQLite suite runs."""
    return [s.where for s in suite_runs(document) if s.key == SQLITE_SUITE]


def _postgres_runs(document: Document) -> list[str]:
    """Where a PostgreSQL suite runs."""
    return [
        s.where
        for s in suite_runs(document)
        if "postgres" in s.features and not s.has_default_features
    ]


def test_the_sqlite_suite_runs_exactly_once(ci: Document) -> None:
    """One run, and it is the coverage job's, which carries the ratchet."""
    assert _sqlite_runs(ci) == ["coverage:Generate coverage for SQLite"]


def test_the_postgres_suite_runs_once_with_legacy_networking(ci: Document) -> None:
    """One PostgreSQL run, at coverage, and it builds `legacy-networking`.

    The `legacy-networking` tests (`tests/integration.rs`,
    `tests/runtime_selection_bdd.rs`) compile only when that feature is on, so
    a PostgreSQL run without it would leave them untested.
    """
    keys = {s.key for s in suite_runs(ci) if "postgres" in s.features}
    assert keys == {POSTGRES_SUITE}
    assert _postgres_runs(ci) == ["coverage:Generate coverage for Postgres"]


def test_the_main_branch_run_builds_the_same_postgres_suite() -> None:
    """`coverage-main.yml` builds what the pull-request run builds."""
    main = repository_documents()["coverage-main.yml"]
    keys = {s.key for s in suite_runs(main) if "postgres" in s.features}
    assert keys == {POSTGRES_SUITE}


def test_no_suite_runs_twice(ci: Document) -> None:
    """Every feature set is run in one place, per event."""
    counts = collections.Counter(s.key for s in suite_runs(ci))
    twice = {tuple(sorted(key[0])): n for key, n in counts.items() if n > 1}
    assert not twice, f"these suites run more than once: {twice}"


def test_every_run_fails_the_job_that_holds_it(ci: Document) -> None:
    """A run whose failure is tolerated would not stand in for a test run."""
    tolerant = [s.where for s in suite_runs(ci) if not s.is_blocking]
    assert not tolerant, f"these runs cannot fail their job: {tolerant}"


def test_the_sqlite_leg_still_lints_the_default_features(ci: Document) -> None:
    """Skipping the leg's tests keeps its Clippy and Whitaker passes."""
    assert not lint_problems(ci)


def _mutated(
    ci: Document, mutate: cabc.Callable[[dict[str, typ.Any]], None]
) -> Document:
    """Return a deep copy of `ci` altered by `mutate`."""
    clone = copy.deepcopy(dict(ci))
    mutate(clone)
    return clone


def _test_step(document: dict[str, typ.Any]) -> dict[str, typ.Any]:
    """The build-test step that runs nextest."""
    steps = document["jobs"]["build-test"]["steps"]
    return next(s for s in steps if NEXTEST_STEP in str(s.get("run", "")))


def test_removing_the_guard_makes_the_suite_run_twice(ci: Document) -> None:
    """Without the `if`, the sqlite leg runs its tests beside the coverage job."""
    doubled = _mutated(ci, lambda d: _test_step(d).pop("if"))
    assert collections.Counter(_sqlite_runs(doubled)) == {
        "build-test:sqlite": 1,
        "coverage:Generate coverage for SQLite": 1,
    }


def test_dropping_the_coverage_step_leaves_no_sqlite_run(ci: Document) -> None:
    """With the guard kept and the coverage step gone, SQLite is untested."""

    def drop(document: dict[str, typ.Any]) -> None:
        job = document["jobs"]["coverage"]
        job["steps"] = [
            s for s in job["steps"] if s.get("name") != "Generate coverage for SQLite"
        ]

    assert _sqlite_runs(_mutated(ci, drop)) == []


def test_dropping_the_postgres_runs_leaves_none(ci: Document) -> None:
    """Both PostgreSQL runs gone is visible to the presence half."""

    def drop(document: dict[str, typ.Any]) -> None:
        job = document["jobs"]["coverage"]
        job["steps"] = [s for s in job["steps"] if "Postgres" not in str(s.get("name"))]
        leg_test = _test_step(document)
        leg_test["if"] = "matrix.name == 'nothing'"

    assert _postgres_runs(_mutated(ci, drop)) == []


def test_an_unrecognized_condition_fails_rather_than_passing() -> None:
    """A condition the query cannot read must not make a leg look skipped."""
    with pytest.raises(AssertionError, match="unsupported"):
        leg_runs_step("contains(matrix.name, 'sql')", "sqlite")


def test_a_tolerated_coverage_step_is_not_blocking(ci: Document) -> None:
    """`continue-on-error` on the surviving SQLite run makes it non-blocking."""

    def tolerate(document: dict[str, typ.Any]) -> None:
        for step in document["jobs"]["coverage"]["steps"]:
            if step.get("name") == "Generate coverage for SQLite":
                step["continue-on-error"] = True

    tolerated = [
        s.where for s in suite_runs(_mutated(ci, tolerate)) if not s.is_blocking
    ]
    assert tolerated == ["coverage:Generate coverage for SQLite"]


def test_a_second_run_of_a_feature_set_is_seen(ci: Document) -> None:
    """The guard deleted, the same feature set appears twice."""
    doubled = _mutated(ci, lambda d: _test_step(d).pop("if"))
    counts = collections.Counter(s.key for s in suite_runs(doubled))
    assert counts[SQLITE_SUITE] == 2
    assert counts[POSTGRES_SUITE] == 2


def test_a_constant_false_condition_removes_the_sqlite_run(ci: Document) -> None:
    """`if: false` on the coverage step leaves a run that never executes."""

    def skip(document: dict[str, typ.Any]) -> None:
        for step in document["jobs"]["coverage"]["steps"]:
            if step.get("name") == "Generate coverage for SQLite":
                step["if"] = "false"

    assert _sqlite_runs(_mutated(ci, skip)) == []


def test_a_conditional_coverage_job_removes_its_runs(ci: Document) -> None:
    """The same, at the job."""
    skipped = _mutated(ci, lambda d: d["jobs"]["coverage"].update({"if": "false"}))
    assert _sqlite_runs(skipped) == []


def test_a_masked_test_command_is_not_blocking(ci: Document) -> None:
    """`|| true` on the nextest command lets failures pass."""

    def mask(document: dict[str, typ.Any]) -> None:
        step = _test_step(document)
        step["run"] = step["run"].replace("--filter-expr", "|| true --filter-expr")

    masked = [s.where for s in suite_runs(_mutated(ci, mask)) if not s.is_blocking]
    assert masked == ["build-test:wireframe-only"]


@pytest.mark.parametrize(
    "mutation",
    ["flags", "clippy-if", "whitaker-flags", "no-leg", "postgres-flags"],
)
def test_lint_problems_are_seen(ci: Document, mutation: str) -> None:
    """Each way of hollowing out a lint-only leg is reported."""

    def mutate(document: dict[str, typ.Any]) -> None:
        job = document["jobs"]["build-test"]
        legs = job["strategy"]["matrix"]["include"]
        sqlite = next(leg for leg in legs if leg["name"] == "sqlite")
        lints = [s for s in job["steps"] if s.get("name", "").startswith("Lint with")]
        if mutation == "flags":
            sqlite["cargo_flags"] = "--no-default-features --features sqlite"
        elif mutation == "clippy-if":
            lints[0]["if"] = "matrix.name != 'sqlite'"
        elif mutation == "whitaker-flags":
            lints[1]["run"] = lints[1]["run"].replace(LINT_FLAGS, "")
        elif mutation == "postgres-flags":
            postgres = next(leg for leg in legs if leg["name"] == "postgres")
            postgres["cargo_flags"] = postgres["cargo_flags"].replace(
                ",legacy-networking", ""
            )
        else:
            legs.remove(sqlite)

    assert lint_problems(_mutated(ci, mutate))
