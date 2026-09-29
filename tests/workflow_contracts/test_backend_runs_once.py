"""Each database backend has a test run in CI, and the SQLite suite runs once.

The build-test sqlite leg once ran the same suite as the coverage job's SQLite
step (`--features sqlite,test-support`, default features on), so every event
paid for it twice. Deduplication is a hazard as well as a saving: removing the
wrong copy leaves a backend untested while every check stays green. The rule
these contracts hold is that both backends are first-class, so neither may lose
its only run, and the SQLite suite must not run again in a second job.

The judgement is a query over a parsed workflow, :func:`test_runs`, that lists
every suite the workflow runs as a feature set. The repository's `ci.yml` is
read through it, and so are mutated copies, so the query is driven against the
defects it exists to catch rather than only matched against a healthy file.
"""

from __future__ import annotations

import collections
import copy
import re
import typing as typ

import pytest
from ci_workflow_reader import repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

    type Document = cabc.Mapping[str, object]

COVERAGE_ACTION: typ.Final = "generate-coverage"
NEXTEST_STEP: typ.Final = "cargo nextest run ${{ matrix.cargo_flags }}"
LEG_CONDITION: typ.Final = re.compile(r"^matrix\.name (==|!=) '([\w-]+)'$")
SQLITE_SUITE: typ.Final = (frozenset({"sqlite", "test-support"}), True)


class Suite(typ.NamedTuple):
    """One test run: where it is declared and which features it builds."""

    where: str
    features: frozenset[str]
    has_default_features: bool
    is_blocking: bool = True

    @property
    def key(self) -> tuple[frozenset[str], bool]:
        """The identity of the suite, independent of where it runs."""
        return (self.features, self.has_default_features)


def _mapping(value: object) -> dict[str, object]:
    """Return `value` when it is a mapping, else an empty one."""
    return value if isinstance(value, dict) else {}


def _steps(job: cabc.Mapping[str, object]) -> list[dict[str, object]]:
    """A job's steps that parse to mappings, in order."""
    declared = job.get("steps")
    return (
        [s for s in declared if isinstance(s, dict)]
        if isinstance(declared, list)
        else []
    )


def _is_blocking(*owners: cabc.Mapping[str, object]) -> bool:
    """Say whether a failure of the owners' run fails the job.

    A step or job declaring `continue-on-error` at all, even `false` written as
    an expression, is treated as tolerant: the contract only trusts the
    absence of the key.
    """
    return all("continue-on-error" not in owner for owner in owners)


def _features(text: str) -> frozenset[str]:
    """Split a feature list written with commas, spaces, or both."""
    return frozenset(part for part in re.split(r"[\s,]+", text.strip()) if part)


def _leg_flags(flags: str) -> tuple[frozenset[str], bool]:
    """Read `--features` and `--no-default-features` from a leg's flags."""
    listed = re.findall(r"--features\s+(\S+)", flags)
    return _features(" ".join(listed)), "--no-default-features" not in flags


def _leg_runs_step(condition: object, name: str) -> bool:
    """Decide whether a step guarded by `condition` runs for the leg `name`.

    Only the two shapes the workflow uses are understood. Anything else fails
    the contract rather than being guessed at, so a cleverer condition cannot
    make a leg look skipped when it runs.
    """
    if condition is None:
        return True
    match = LEG_CONDITION.match(str(condition))
    assert match, f"unsupported build-test step condition: {condition!r}"
    operator, leg = match.groups()
    return (name == leg) == (operator == "==")


def _build_test_runs(document: Document) -> list[Suite]:
    """Suites the build-test matrix runs, one per leg whose test step runs."""
    job = _mapping(_mapping(document.get("jobs")).get("build-test"))
    legs = _mapping(_mapping(job.get("strategy")).get("matrix")).get("include")
    if not isinstance(legs, list):
        return []
    tests = [s for s in _steps(job) if NEXTEST_STEP in str(s.get("run", ""))]
    return [
        Suite(
            f"build-test:{leg['name']}",
            *_leg_flags(str(leg.get("cargo_flags", ""))),
            _is_blocking(job, step),
        )
        for leg in legs
        if isinstance(leg, dict)
        for step in tests
        if _leg_runs_step(step.get("if"), str(leg.get("name")))
    ]


def _coverage_runs(document: Document) -> list[Suite]:
    """Suites the coverage steps run, whichever job declares them."""
    runs: list[Suite] = []
    for job_id, job in _mapping(document.get("jobs")).items():
        for step in _steps(_mapping(job)):
            if COVERAGE_ACTION not in str(step.get("uses", "")):
                continue
            inputs = _mapping(step.get("with"))
            # YAML reads an unquoted `false` as a boolean, and a quoted one as text.
            has_defaults = (
                str(inputs.get("with-default-features", "true")).lower() != "false"
            )
            runs.append(
                Suite(
                    f"{job_id}:{step.get('name')}",
                    _features(str(inputs.get("features", ""))),
                    has_defaults,
                    _is_blocking(_mapping(job), step),
                )
            )
    return runs


def test_runs(document: Document) -> list[Suite]:
    """List every suite a workflow runs, from build-test and coverage steps."""
    return [*_build_test_runs(document), *_coverage_runs(document)]


test_runs.__test__ = False  # type: ignore[attr-defined]  # a query, not a test


@pytest.fixture(scope="module")
def ci() -> Document:
    """This repository's `ci.yml`."""
    return repository_documents()["ci.yml"]


def _sqlite_runs(document: Document) -> list[str]:
    """Where the default-feature SQLite suite runs."""
    return [s.where for s in test_runs(document) if s.key == SQLITE_SUITE]


def _postgres_runs(document: Document) -> list[str]:
    """Where a PostgreSQL suite runs."""
    return [
        s.where
        for s in test_runs(document)
        if "postgres" in s.features and not s.has_default_features
    ]


def test_the_sqlite_suite_runs_exactly_once(ci: Document) -> None:
    """One run, and it is the coverage job's, which carries the ratchet."""
    assert _sqlite_runs(ci) == ["coverage:Generate coverage for SQLite"]


def test_the_postgres_suites_each_have_a_run(ci: Document) -> None:
    """Both PostgreSQL feature sets run: coverage's and build-test's.

    The build-test leg also builds `legacy-networking`, which the coverage
    step does not, so the two are different suites and neither duplicates the
    other. Losing the leg would leave those tests uncompiled under PostgreSQL.
    """
    keys = {s.key for s in test_runs(ci) if "postgres" in s.features}
    assert keys == {
        (frozenset({"postgres", "test-support"}), False),
        (frozenset({"postgres", "test-support", "legacy-networking"}), False),
    }


def test_no_suite_runs_twice(ci: Document) -> None:
    """Every feature set is run in one place, per event."""
    counts = collections.Counter(s.key for s in test_runs(ci))
    twice = {tuple(sorted(key[0])): n for key, n in counts.items() if n > 1}
    assert not twice, f"these suites run more than once: {twice}"


def test_every_run_fails_the_job_that_holds_it(ci: Document) -> None:
    """A run whose failure is tolerated would not stand in for a test run."""
    tolerant = [s.where for s in test_runs(ci) if not s.is_blocking]
    assert not tolerant, f"these runs cannot fail their job: {tolerant}"


def test_the_sqlite_leg_still_lints_the_default_features(ci: Document) -> None:
    """Skipping the leg's tests keeps its Clippy and Whitaker passes."""
    job = _mapping(_mapping(ci.get("jobs")).get("build-test"))
    legs = _mapping(_mapping(job.get("strategy")).get("matrix")).get("include")
    names = [leg.get("name") for leg in legs if isinstance(leg, dict)]
    assert "sqlite" in names, "the sqlite leg carries the default-feature lint"
    lints = [s for s in _steps(job) if str(s.get("name", "")).startswith("Lint with")]
    assert len(lints) == 2, f"expected the Clippy and Whitaker steps, got {lints}"
    assert all("if" not in step for step in lints), "a lint step must not be skipped"


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
        _leg_runs_step("contains(matrix.name, 'sql')", "sqlite")


def test_a_tolerated_coverage_step_is_not_blocking(ci: Document) -> None:
    """`continue-on-error` on the surviving SQLite run makes it non-blocking."""

    def tolerate(document: dict[str, typ.Any]) -> None:
        for step in document["jobs"]["coverage"]["steps"]:
            if step.get("name") == "Generate coverage for SQLite":
                step["continue-on-error"] = True

    tolerated = [
        s.where for s in test_runs(_mutated(ci, tolerate)) if not s.is_blocking
    ]
    assert tolerated == ["coverage:Generate coverage for SQLite"]


def test_a_second_run_of_a_feature_set_is_seen(ci: Document) -> None:
    """The guard deleted, the same feature set appears twice."""
    doubled = _mutated(ci, lambda d: _test_step(d).pop("if"))
    counts = collections.Counter(s.key for s in test_runs(doubled))
    assert counts[SQLITE_SUITE] == 2
