"""Every lane that moved to Ubicloud records where it ran in its job summary.

A lane's placement and cache state decide what it costs and how long it takes,
and a lane that moved should be readable without opening its log. The last step
of each such lane writes the lane, the runner, the event, the fork flag, the
cache hit and the status to the job summary under ``always()``, so a failing
lane is recorded too. Queue wait and duration cannot be known from inside a
job; the jobs API supplies them.
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile
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
#: The exact expression each input must bind, for every lane. The cache input
#: differs by lane: docs-tooling reports its Merman CLI cache, the others the
#: Rust cache.
COMMON_ENV: typ.Final = {
    "LANE": "${{ github.job }}",
    "RUNNER_ENVIRONMENT": "${{ runner.environment }}",
    "RUNNER_LABEL": "${{ runner.name }}",
    "EVENT": "${{ github.event_name }}",
    "FORK": "${{ github.event.pull_request.head.repo.fork || false }}",
    "JOB_STATUS": "${{ job.status }}",
}
CACHE_STEP: typ.Final = {
    "docs-tooling": "cache-merman-cli",
    "build-test": "rust-cache",
    "validator-sqlite": "rust-cache",
    "coverage": "rust-cache",
}
#: The one lane that is a matrix, and the input naming its leg.
MATRIX_LANE: typ.Final = "build-test"
MATRIX_LEG: typ.Final = "${{ matrix.name }}"


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
    assert "GITHUB_STEP_SUMMARY" in str(step.get("run")), (
        f"{lane}: the record must write to the job summary"
    )


def _expected_env(lane: str) -> dict[str, str]:
    """Return the environment a lane's record must bind, expression by expression.

    Parameters
    ----------
    lane
        The job name.

    Returns
    -------
    dict[str, str]
        Input name to the exact expression it must hold.
    """
    expected = {
        **COMMON_ENV,
        "CACHE_HIT": f"${{{{ steps.{CACHE_STEP[lane]}.outputs.cache-hit }}}}",
    }
    if lane == MATRIX_LANE:
        expected["MATRIX_LEG"] = MATRIX_LEG
    return expected


def _step_ids(lane: str) -> set[object]:
    """Return the ids of a lane's steps, so a cache reference can be checked.

    Parameters
    ----------
    lane
        The job name.

    Returns
    -------
    set[object]
        Every step id the lane declares.
    """
    jobs = repository_documents()["ci.yml"]["jobs"]
    return {step.get("id") for step in jobs[lane]["steps"]}


@pytest.mark.parametrize("lane", RECORDED_LANES)
def test_each_input_binds_its_exact_expression(lane: str) -> None:
    """Bind each input to the expression it names, and no other.

    A presence check accepts a wrong fork expression or a cache reference to a
    step the lane does not have, which would write a confident, false summary.

    Parameters
    ----------
    lane
        The ``ci.yml`` job to read.
    """
    environment = _last_step(lane).get("env")
    assert environment == _expected_env(lane), (
        f"{lane}: the record binds {environment}, expected {_expected_env(lane)}"
    )
    assert CACHE_STEP[lane] in _step_ids(lane), (
        f"{lane}: the cache input reads step {CACHE_STEP[lane]!r}, which it lacks"
    )
    script = str(_last_step(lane).get("run"))
    unused = [
        name
        for name in _expected_env(lane)
        if f"${{{name}" not in script and f"${name}" not in script
    ]
    assert not unused, f"{lane}: the record never reads {unused}"


@pytest.mark.parametrize("lane", RECORDED_LANES)
def test_the_record_renders_every_field_into_the_summary(lane: str) -> None:
    """Run the step's script and read what it writes, not only what it names.

    Parameters
    ----------
    lane
        The ``ci.yml`` job to read.
    """
    script = str(_last_step(lane)["run"])
    values = {
        "LANE": lane,
        "RUNNER_LABEL": "runner-7",
        "RUNNER_ENVIRONMENT": "self-hosted",
        "EVENT": "pull_request",
        "FORK": "false",
        "JOB_STATUS": "success",
        "CACHE_HIT": "true",
        "MATRIX_LEG": "sqlite",
    }
    with tempfile.TemporaryDirectory() as scratch:
        summary = pathlib.Path(scratch) / "summary.md"
        subprocess.run(  # noqa: S603 - fixed argument vector over repository text
            ["bash", "-euo", "pipefail", "-c", script],
            env={**os.environ, **values, "GITHUB_STEP_SUMMARY": str(summary)},
            check=True,
        )
        rendered = summary.read_text(encoding="utf-8")
    for fragment in (
        f"lane: {lane}",
        "runner-7 (self-hosted)",
        "event: pull_request, fork: false",
        "exact cache hit: true",
        "status so far: success",
    ):
        assert fragment in rendered, (
            f"{lane}: the summary lacks {fragment!r}:\n{rendered}"
        )
    if lane == MATRIX_LANE:
        assert "(sqlite)" in rendered, f"{lane}: the summary omits its matrix leg"
