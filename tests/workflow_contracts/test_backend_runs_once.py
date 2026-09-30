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
    runs = _sqlite_runs(ci)
    assert runs == ["coverage:Generate coverage for SQLite"], (
        f"the SQLite suite must run once, at coverage; it runs at {runs}"
    )


def test_the_postgres_suite_runs_once_with_legacy_networking(ci: Document) -> None:
    """One PostgreSQL run, at coverage, and it builds `legacy-networking`.

    The `legacy-networking` tests (`tests/integration.rs`,
    `tests/runtime_selection_bdd.rs`) compile only when that feature is on, so
    a PostgreSQL run without it would leave them untested.
    """
    keys = {s.key for s in suite_runs(ci) if "postgres" in s.features}
    assert keys == {POSTGRES_SUITE}, (
        f"PostgreSQL feature sets run: {sorted(map(sorted, (k[0] for k in keys)))}"
    )
    runs = _postgres_runs(ci)
    assert runs == ["coverage:Generate coverage for Postgres"], (
        f"the PostgreSQL suite must run once, at coverage; it runs at {runs}"
    )


def test_the_main_branch_run_builds_the_same_postgres_suite() -> None:
    """`coverage-main.yml` builds what the pull-request run builds."""
    main = repository_documents()["coverage-main.yml"]
    keys = {s.key for s in suite_runs(main) if "postgres" in s.features}
    assert keys == {POSTGRES_SUITE}, (
        "coverage-main.yml must build the PostgreSQL suite ci.yml builds, "
        f"found {sorted(map(sorted, (k[0] for k in keys)))}"
    )


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
    problems = lint_problems(ci)
    assert not problems, f"the lint-only legs are hollowed out: {problems}"


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
    seen = collections.Counter(_sqlite_runs(doubled))
    assert seen == {
        "build-test:sqlite": 1,
        "coverage:Generate coverage for SQLite": 1,
    }, f"deleting the guard must show the suite twice, saw {dict(seen)}"


def test_dropping_the_coverage_step_leaves_no_sqlite_run(ci: Document) -> None:
    """With the guard kept and the coverage step gone, SQLite is untested."""

    def drop(document: dict[str, typ.Any]) -> None:
        job = document["jobs"]["coverage"]
        job["steps"] = [
            s for s in job["steps"] if s.get("name") != "Generate coverage for SQLite"
        ]

    assert _sqlite_runs(_mutated(ci, drop)) == [], "no SQLite run may remain"


def test_dropping_the_postgres_runs_leaves_none(ci: Document) -> None:
    """Both PostgreSQL runs gone is visible to the presence half."""

    def drop(document: dict[str, typ.Any]) -> None:
        job = document["jobs"]["coverage"]
        job["steps"] = [s for s in job["steps"] if "Postgres" not in str(s.get("name"))]
        leg_test = _test_step(document)
        leg_test["if"] = "matrix.name == 'nothing'"

    assert _postgres_runs(_mutated(ci, drop)) == [], "no PostgreSQL run may remain"


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
    assert tolerated == ["coverage:Generate coverage for SQLite"], (
        f"only the tolerated SQLite step is non-blocking, saw {tolerated}"
    )


def test_a_second_run_of_a_feature_set_is_seen(ci: Document) -> None:
    """The guard deleted, the same feature set appears twice."""
    doubled = _mutated(ci, lambda d: _test_step(d).pop("if"))
    counts = collections.Counter(s.key for s in suite_runs(doubled))
    assert counts[SQLITE_SUITE] == 2, "the SQLite suite must appear twice"
    assert counts[POSTGRES_SUITE] == 2, "the PostgreSQL suite must appear twice"


def test_a_constant_false_condition_removes_the_sqlite_run(ci: Document) -> None:
    """`if: false` on the coverage step leaves a run that never executes."""

    def skip(document: dict[str, typ.Any]) -> None:
        for step in document["jobs"]["coverage"]["steps"]:
            if step.get("name") == "Generate coverage for SQLite":
                step["if"] = "false"

    assert _sqlite_runs(_mutated(ci, skip)) == [], "a skipped run must not count"


def test_a_conditional_coverage_job_removes_its_runs(ci: Document) -> None:
    """The same, at the job."""
    skipped = _mutated(ci, lambda d: d["jobs"]["coverage"].update({"if": "false"}))
    assert _sqlite_runs(skipped) == [], "a conditional job must not count"


def test_a_masked_test_command_is_not_blocking(ci: Document) -> None:
    """`|| true` on the nextest command lets failures pass."""

    def mask(document: dict[str, typ.Any]) -> None:
        step = _test_step(document)
        step["run"] = step["run"].replace("--filter-expr", "|| true --filter-expr")

    masked = [s.where for s in suite_runs(_mutated(ci, mask)) if not s.is_blocking]
    assert masked == ["build-test:wireframe-only"], f"masked runs seen: {masked}"


def _leg(document: dict[str, typ.Any], name: str) -> dict[str, typ.Any]:
    """The build-test matrix leg called `name`."""
    legs = document["jobs"]["build-test"]["strategy"]["matrix"]["include"]
    return next(leg for leg in legs if leg["name"] == name)


def _lint_step(document: dict[str, typ.Any], index: int) -> dict[str, typ.Any]:
    """The first (Clippy) or second (Whitaker) lint step of build-test."""
    steps = document["jobs"]["build-test"]["steps"]
    return [s for s in steps if s.get("name", "").startswith("Lint with")][index]


def _narrow_sqlite(document: dict[str, typ.Any]) -> None:
    """Build the sqlite leg without default features."""
    _leg(document, "sqlite")["cargo_flags"] = "--no-default-features --features sqlite"


def _narrow_postgres(document: dict[str, typ.Any]) -> None:
    """Build the postgres leg without `legacy-networking`."""
    leg = _leg(document, "postgres")
    leg["cargo_flags"] = leg["cargo_flags"].replace(",legacy-networking", "")


def _skip_clippy(document: dict[str, typ.Any]) -> None:
    """Skip the Clippy step on the sqlite leg."""
    _lint_step(document, 0)["if"] = "matrix.name != 'sqlite'"


def _unflag_whitaker(document: dict[str, typ.Any]) -> None:
    """Drop the feature flags from the Whitaker step."""
    step = _lint_step(document, 1)
    step["run"] = step["run"].replace(LINT_FLAGS, "")


def _drop_sqlite_leg(document: dict[str, typ.Any]) -> None:
    """Remove the sqlite leg from the matrix."""
    legs = document["jobs"]["build-test"]["strategy"]["matrix"]["include"]
    legs.remove(_leg(document, "sqlite"))


@pytest.mark.parametrize(
    "mutate",
    [
        _narrow_sqlite,
        _narrow_postgres,
        _skip_clippy,
        _unflag_whitaker,
        _drop_sqlite_leg,
    ],
)
def test_lint_problems_are_seen(
    ci: Document, mutate: cabc.Callable[[dict[str, typ.Any]], None]
) -> None:
    """Each way of hollowing out a lint-only leg is reported."""
    assert lint_problems(_mutated(ci, mutate)), f"{mutate.__name__} went unseen"
