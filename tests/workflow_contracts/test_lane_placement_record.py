"""Every lane that moved to Ubicloud records where it ran in its job summary.

A lane's placement and cache state decide what it costs and how long it takes,
and a lane that moved should be readable without opening its log. The last step
of each such lane writes the lane, the runner, the event, the fork flag, the
cache hit and the status to the job summary under ``always()``, so a failing
lane is recorded too. Queue wait and duration cannot be known from inside a
job; the jobs API supplies them.
"""

from __future__ import annotations

import typing as typ

import pytest
from ci_workflow_reader import repository_documents

#: The ``ci.yml`` lanes that run on Ubicloud and so record their placement.
RECORDED_LANES: typ.Final = (
    "docs-tooling",
    "build-test",
    "validator-sqlite",
    "coverage",
)
STEP_NAME: typ.Final = "Record the lane's placement"
#: What the step must carry, as the values it reads from the run.
REQUIRED_ENV: typ.Final = (
    "LANE",
    "RUNNER_ENVIRONMENT",
    "RUNNER_LABEL",
    "EVENT",
    "FORK",
    "JOB_STATUS",
    "CACHE_HIT",
)


def _last_step(lane: str) -> dict[str, object]:
    """Return a lane's last step from the real ``ci.yml``.

    Parameters
    ----------
    lane
        The job name.

    Returns
    -------
    dict[str, object]
        The step mapping.
    """
    document = repository_documents()["ci.yml"]
    jobs = document["jobs"]
    assert isinstance(jobs, dict), "ci.yml has no jobs mapping"
    steps = jobs[lane]["steps"]
    assert isinstance(steps, list), f"{lane} has no steps"
    return steps[-1]


@pytest.mark.parametrize("lane", RECORDED_LANES)
def test_a_moved_lane_ends_by_recording_its_placement(lane: str) -> None:
    """End each lane with an ``always()`` step that writes the job summary.

    Parameters
    ----------
    lane
        The ``ci.yml`` job to read.
    """
    step = _last_step(lane)
    assert step.get("name") == STEP_NAME, (
        f"{lane} must end with {STEP_NAME!r}, not {step.get('name')!r}"
    )
    assert step.get("if") == "${{ always() }}", (
        f"{lane}: the record must run on failure too, found {step.get('if')!r}"
    )
    environment = step.get("env")
    assert isinstance(environment, dict), f"{lane}: the record reads no environment"
    missing = [name for name in REQUIRED_ENV if name not in environment]
    assert not missing, f"{lane}: the record omits {missing}"
    assert "GITHUB_STEP_SUMMARY" in str(step.get("run")), (
        f"{lane}: the record must write to the job summary"
    )
