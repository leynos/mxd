"""The ruleset's `build-test` context fails whenever any matrix leg does.

The ruleset cannot require the matrix legs directly: GitHub truncates their
check names and they change with the matrix, so a required leg would either
go stale or block every pull request. `build-test-result` is the one stable
context it requires instead, and it is only a gate if three things hold:

- it depends on `build-test`, so it reads that job's aggregate result;
- it runs under `always()`, because a job skipped after a failed dependency
  reports "skipped", and GitHub counts a skipped required check as passing;
- its sole step fails on every result other than `success`, so a skipped or
  cancelled matrix fails it too, and it carries no `continue-on-error` or
  step `if` that could discard or skip that verdict.

The judgement is :func:`result_job_defects`, driven below with constructed
jobs as well as with this repository's, because the repository's job
exercises only the accepted shape.
"""

from __future__ import annotations

import typing as typ

import pytest
from ci_workflow_reader import repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

RESULT_JOB: typ.Final = "build-test-result"
MATRIX_JOB: typ.Final = "build-test"
# The whole command, compared by equality: `!= failure` would pass a skipped
# or cancelled matrix, and a body that merely contains the test could ignore
# its status.
RESULT_COMMAND: typ.Final = 'test "${{ needs.build-test.result }}" = success'


def result_job_defects(job: cabc.Mapping[str, object]) -> tuple[str, ...]:
    """Describe how an aggregate job fails to gate on the matrix.

    Returns
    -------
    tuple[str, ...]
        One sentence per defect, empty when the job gates as required.
    """
    defects: list[str] = []
    needs = job.get("needs")
    if needs not in (MATRIX_JOB, [MATRIX_JOB]):
        defects.append(f"needs must be {MATRIX_JOB!r}, not {needs!r}")
    if job.get("if") != "always()":
        defects.append(f"if must be 'always()', not {job.get('if')!r}")
    if "continue-on-error" in job:
        defects.append("the job must not carry continue-on-error")
    steps = job.get("steps")
    if not (isinstance(steps, list) and len(steps) == 1 and isinstance(steps[0], dict)):
        return (*defects, "the job must have exactly one step")
    (step,) = steps
    if str(step.get("run", "")).strip() != RESULT_COMMAND:
        defects.append(f"its step must run exactly {RESULT_COMMAND!r}")
    defects.extend(
        f"its step must not carry {key}"
        for key in ("if", "continue-on-error", "shell", "working-directory")
        if key in step
    )
    return tuple(defects)


@pytest.fixture(scope="module")
def documents() -> cabc.Mapping[str, cabc.Mapping[str, object]]:
    """This repository's parsed workflows."""
    return repository_documents()


def test_the_result_job_gates_on_the_matrix(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """ci.yml's aggregate job depends on, runs after, and fails with the matrix."""
    jobs = documents["ci.yml"].get("jobs")
    assert isinstance(jobs, dict), "ci.yml must declare jobs"
    job = jobs.get(RESULT_JOB)
    assert isinstance(job, dict), f"ci.yml must declare {RESULT_JOB}"
    assert result_job_defects(job) == ()


GATING_JOB: typ.Final = {
    "if": "always()",
    "needs": MATRIX_JOB,
    "steps": [{"run": RESULT_COMMAND}],
}


@pytest.mark.parametrize(
    ("change", "expected"),
    [
        pytest.param({"if": None}, "if must be", id="no-always"),
        pytest.param({"if": "success()"}, "if must be", id="success-only"),
        pytest.param({"needs": "coverage"}, "needs must be", id="wrong-dependency"),
        pytest.param({"continue-on-error": True}, "continue-on-error", id="job-soft"),
        pytest.param(
            {"steps": [{"run": 'test "${{ needs.build-test.result }}" != failure'}]},
            "must run exactly",
            id="passes-skipped",
        ),
        pytest.param(
            {"steps": [{"run": RESULT_COMMAND, "continue-on-error": True}]},
            "must not carry continue-on-error",
            id="step-soft",
        ),
        pytest.param({"steps": []}, "exactly one step", id="no-step"),
    ],
)
def test_a_job_that_would_pass_a_failed_matrix_is_refused(
    change: dict[str, object], expected: str
) -> None:
    """Each weakened shape is reported, not accepted."""
    job = {
        key: value
        for key, value in {**GATING_JOB, **change}.items()
        if value is not None
    }
    assert any(expected in defect for defect in result_job_defects(job))


def test_the_gating_shape_is_accepted() -> None:
    """The narrowness half: the required shape yields no defect."""
    assert result_job_defects(GATING_JOB) == ()
