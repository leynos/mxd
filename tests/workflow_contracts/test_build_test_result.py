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


# Execution overrides that change how the step's command runs. A shell
# template such as `bash {0} || true` masks the command's status, and a
# directory runs a different tree.
OVERRIDES: typ.Final = ("shell", "working-directory")


def _job_defects(job: cabc.Mapping[str, object]) -> list[str]:
    """How the job itself fails to depend on and run after the matrix."""
    needs = job.get("needs")
    checks = (
        (
            needs in (MATRIX_JOB, [MATRIX_JOB]),
            f"needs must be {MATRIX_JOB!r}, not {needs!r}",
        ),
        (job.get("if") == "always()", f"if must be 'always()', not {job.get('if')!r}"),
        ("continue-on-error" not in job, "the job must not carry continue-on-error"),
        (
            job.get("permissions") == {},
            "the job must declare permissions: {}, since it reads only the result",
        ),
    )
    return [message for holds, message in checks if not holds]


def _run_defaults_defects(scope: str, owner: cabc.Mapping[str, object]) -> list[str]:
    """Execution overrides a `defaults.run` block at one scope would impose."""
    match owner.get("defaults"):
        case {"run": dict() as run}:
            return [
                f"{scope} defaults.run must not set {key}"
                for key in OVERRIDES
                if key in run
            ]
        case _:
            return []


def _step_defects(step: cabc.Mapping[str, object]) -> list[str]:
    """How the job's one step fails to reject every non-success result."""
    defects = [
        f"its step must not carry {key}"
        for key in ("if", "continue-on-error", *OVERRIDES)
        if key in step
    ]
    if str(step.get("run", "")).strip() != RESULT_COMMAND:
        defects.append(f"its step must run exactly {RESULT_COMMAND!r}")
    return defects


def result_job_defects(
    job: cabc.Mapping[str, object],
    workflow: cabc.Mapping[str, object] | None = None,
) -> tuple[str, ...]:
    """Describe how an aggregate job fails to gate on the matrix.

    Parameters
    ----------
    job
        The parsed aggregate job.
    workflow
        The parsed workflow declaring it, read for a workflow-level
        `defaults.run` override. Omitted when judging a job alone.

    Returns
    -------
    tuple[str, ...]
        One sentence per defect, empty when the job gates as required.
    """
    match job.get("steps"):
        case [dict() as step]:
            step_defects = _step_defects(step)
        case _:
            step_defects = ["the job must have exactly one step"]
    return (
        *_job_defects(job),
        *_run_defaults_defects("the job's", job),
        *_run_defaults_defects("the workflow's", workflow or {}),
        *step_defects,
    )


@pytest.fixture(scope="module")
def documents() -> cabc.Mapping[str, cabc.Mapping[str, object]]:
    """This repository's parsed workflows."""
    return repository_documents()


def test_the_result_job_gates_on_the_matrix(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """ci.yml's aggregate job depends on, runs after, and fails with the matrix."""
    workflow = documents["ci.yml"]
    match workflow.get("jobs"):
        case {"build-test-result": dict() as job}:
            pass
        case _:
            pytest.fail(f"ci.yml must declare {RESULT_JOB} as a mapping")
    defects = result_job_defects(job, workflow)
    assert defects == (), f"{RESULT_JOB} does not gate on the matrix: {defects}"


GATING_JOB: typ.Final = {
    "if": "always()",
    "needs": MATRIX_JOB,
    "permissions": {},
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
        pytest.param({"permissions": None}, "permissions", id="default-token"),
        pytest.param(
            {"defaults": {"run": {"shell": "bash {0} || true"}}},
            "defaults.run must not set shell",
            id="job-shell",
        ),
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
    defects = result_job_defects(job)
    assert any(expected in defect for defect in defects), (
        f"expected a defect naming {expected!r}, found {defects}"
    )


def test_a_workflow_shell_override_is_refused() -> None:
    """A workflow-level `defaults.run.shell` reaches the step as well."""
    workflow = {"defaults": {"run": {"shell": "bash {0} || true"}}}
    defects = result_job_defects(GATING_JOB, workflow)
    assert any("the workflow's defaults.run" in defect for defect in defects), (
        f"a workflow shell override must be refused, found {defects}"
    )


def test_the_gating_shape_is_accepted() -> None:
    """The narrowness half: the required shape yields no defect."""
    defects = result_job_defects(GATING_JOB, {})
    assert defects == (), f"the gating shape must pass, found {defects}"
