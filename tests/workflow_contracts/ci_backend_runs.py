"""Query over a parsed `ci.yml` for the test suites it runs and how they lint.

Each suite is a feature set (the `Suite` record), read from the build-test
matrix legs whose test step runs and from every `generate-coverage` step. The
judgement lives here, apart from the contract that applies it, so the
contracts can drive it with constructed and mutated copies of the workflow.
See `docs/developers-guide.md`, "Each backend runs once per event".
"""

from __future__ import annotations

import re
import typing as typ

if typ.TYPE_CHECKING:
    import collections.abc as cabc

    type Document = cabc.Mapping[str, object]

COVERAGE_ACTION: typ.Final = "generate-coverage"
NEXTEST_STEP: typ.Final = "cargo nextest run ${{ matrix.cargo_flags }}"
LEG_CONDITION: typ.Final = re.compile(r"^matrix\.name (==|!=) '([\w-]+)'$")
SQLITE_SUITE: typ.Final = (frozenset({"sqlite", "test-support"}), True)
POSTGRES_SUITE: typ.Final = (
    frozenset({"postgres", "test-support", "legacy-networking"}),
    False,
)


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


def mapping(value: object) -> dict[str, object]:
    """Return `value` when it is a mapping, else an empty one."""
    return value if isinstance(value, dict) else {}


def steps(job: cabc.Mapping[str, object]) -> list[dict[str, object]]:
    """A job's steps that parse to mappings, in order."""
    declared = job.get("steps")
    return (
        [s for s in declared if isinstance(s, dict)]
        if isinstance(declared, list)
        else []
    )


MASKING: typ.Final = re.compile(
    r"\|\|\s*(true|:|exit\s+0)\b|;\s*exit\s+0\b|\bset\s+\+e\b", re.IGNORECASE
)


def _is_blocking(*owners: cabc.Mapping[str, object]) -> bool:
    """Say whether a failure of the owners' run fails the job.

    A step or job declaring `continue-on-error` at all, even `false` written as
    an expression, is treated as tolerant: the contract only trusts the
    absence of the key. A `run` body that swallows the command's status, such
    as a trailing `|| true`, is tolerant for the same reason.
    """
    return all(
        "continue-on-error" not in owner
        and not MASKING.search(str(owner.get("run", "")))
        for owner in owners
    )


def may_be_skipped(*owners: cabc.Mapping[str, object]) -> bool:
    """Say whether any owner carries a condition that could skip its run.

    An unconditional owner is the only one the contract counts. A condition
    such as `if: false` would otherwise leave a run in the workflow that never
    executes, and reading conditions to find out which ones are constant would
    be a second, weaker workflow evaluator.
    """
    return any("if" in owner for owner in owners)


def _features(text: str) -> frozenset[str]:
    """Split a feature list written with commas, spaces, or both."""
    return frozenset(part for part in re.split(r"[\s,]+", text.strip()) if part)


def _leg_flags(flags: str) -> tuple[frozenset[str], bool]:
    """Read `--features` and `--no-default-features` from a leg's flags."""
    listed = re.findall(r"--features\s+(\S+)", flags)
    return _features(" ".join(listed)), "--no-default-features" not in flags


def leg_runs_step(condition: object, name: str) -> bool:
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


def _build_suite_runs(document: Document) -> list[Suite]:
    """Suites the build-test matrix runs, one per leg whose test step runs."""
    job = mapping(mapping(document.get("jobs")).get("build-test"))
    legs = mapping(mapping(job.get("strategy")).get("matrix")).get("include")
    if not isinstance(legs, list):
        return []
    if may_be_skipped(job):
        return []
    tests = [s for s in steps(job) if NEXTEST_STEP in str(s.get("run", ""))]
    return [
        Suite(
            f"build-test:{leg['name']}",
            *_leg_flags(str(leg.get("cargo_flags", ""))),
            _is_blocking(job, step),
        )
        for leg in legs
        if isinstance(leg, dict)
        for step in tests
        if leg_runs_step(step.get("if"), str(leg.get("name")))
    ]


def _coverage_runs(document: Document) -> list[Suite]:
    """Suites the coverage steps run, whichever job declares them."""
    runs: list[Suite] = []
    for job_id, job in mapping(document.get("jobs")).items():
        for step in steps(mapping(job)):
            if COVERAGE_ACTION not in str(step.get("uses", "")):
                continue
            if may_be_skipped(mapping(job), step):
                continue
            inputs = mapping(step.get("with"))
            # YAML reads an unquoted `false` as a boolean, and a quoted one as text.
            has_defaults = (
                str(inputs.get("with-default-features", "true")).lower() != "false"
            )
            runs.append(
                Suite(
                    f"{job_id}:{step.get('name')}",
                    _features(str(inputs.get("features", ""))),
                    has_defaults,
                    _is_blocking(mapping(job), step),
                )
            )
    return runs


def suite_runs(document: Document) -> list[Suite]:
    """List every suite a workflow runs, from build-test and coverage steps."""
    return [*_build_suite_runs(document), *_coverage_runs(document)]


LINT_FLAGS: typ.Final = "${{ matrix.cargo_flags }}"
# The legs that stay in the matrix only to lint a feature set nothing else
# lints, since the coverage job runs their tests.
LINT_LEG_FLAGS: typ.Final = {
    "sqlite": "--features sqlite,test-support",
    "postgres": (
        "--no-default-features --features postgres,test-support,legacy-networking"
    ),
}


def _leg_problems(job: cabc.Mapping[str, object]) -> list[str]:
    """Problems with the lint-only legs' presence and feature sets."""
    legs = mapping(mapping(job.get("strategy")).get("matrix")).get("include")
    named = [leg for leg in legs or () if isinstance(leg, dict)]
    problems: list[str] = []
    for name, expected in LINT_LEG_FLAGS.items():
        found = [leg for leg in named if leg.get("name") == name]
        if len(found) != 1:
            problems.append(f"expected one {name} leg, found {len(found)}")
        elif str(found[0].get("cargo_flags", "")).strip() != expected:
            problems.append(f"the {name} leg must build {expected!r}")
    return problems


def _step_problems(job: cabc.Mapping[str, object]) -> list[str]:
    """Problems with the Clippy and Whitaker steps."""
    lints = [s for s in steps(job) if str(s.get("name", "")).startswith("Lint with")]
    problems = (
        [f"expected the Clippy and Whitaker steps, got {len(lints)}"]
        if len(lints) != 2
        else []
    )
    for step in lints:
        if "if" in step:
            problems.append(f"{step.get('name')!r} must not be conditional")
        if LINT_FLAGS not in str(step.get("run", "")):
            problems.append(f"{step.get('name')!r} must lint {LINT_FLAGS}")
    return problems


def lint_problems(document: Document) -> list[str]:
    """Say what stops the lint-only legs linting their feature sets."""
    job = mapping(mapping(document.get("jobs")).get("build-test"))
    conditional = ["the build-test job is conditional"] if may_be_skipped(job) else []
    return [*conditional, *_leg_problems(job), *_step_problems(job)]
