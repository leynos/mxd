"""Each gate this repository claims to run, asserted as a step that runs it.

A gate is a promise that a class of defect cannot reach ``main``. The promise
is kept by a step, and a step keeps it only when three things hold: its whole
``run`` body is the gate command, the step carries no ``if``, and neither it
nor its job carries ``continue-on-error``.

Whole-value equality is the point. A step whose body merely *contains*
``make check-fmt`` is satisfied by ``make check-fmt || true``, by
``make check-fmt &``, and by a body that runs the gate and then exits zero
regardless. Each of those reads as a gate in a diff and gates nothing, which is
worse than having no step at all, because the absence would be noticed.

A condition is judged by key presence rather than by value, because a reader
that coerced ``if`` to text would report the YAML boolean ``false`` as an empty
string and call the step unconditional.
"""

from __future__ import annotations

import typing as typ

import pytest
from ci_workflow_jobs import job_by_coordinate
from ci_workflow_reader import repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

    from ci_workflow_jobs import JobRecord

# Each gate, as (workflow, job, the whole run body the step must carry).
PINNED_GATES: typ.Final[tuple[tuple[str, str, str], ...]] = (
    ("ci.yml", "build-test", "make check-fmt"),
    ("ci.yml", "docs-tooling", "make test-workflow-contracts"),
    ("ci.yml", "docs-tooling", "make spelling"),
    ("ci.yml", "docs-tooling", "make nixie"),
    ("ci.yml", "validator-sqlite", "make test-validator-sqlite"),
)


@pytest.fixture(scope="module")
def documents() -> cabc.Mapping[str, cabc.Mapping[str, object]]:
    """This repository's parsed workflows."""
    return repository_documents()


def _gating_steps(job: JobRecord, command: str) -> tuple[int, ...]:
    """The indices of every step in a job that unconditionally runs a command.

    Parameters
    ----------
    job
        The job to search.
    command
        The whole ``run`` body a step must carry. Compared after stripping
        trailing whitespace only, because a block scalar ends in a newline the
        author did not write and cannot see.

    Returns
    -------
    tuple[int, ...]
        The positions of the gating steps, empty when the job has none.
    """
    return tuple(
        step.index
        for step in job.steps
        if step.run is not None
        and step.run.strip() == command
        and not step.has_condition
        and not step.has_continue_on_error
    )


@pytest.mark.parametrize(
    ("workflow", "job_id", "command"),
    PINNED_GATES,
    ids=[f"{w}:{j}:{c}" for w, j, c in PINNED_GATES],
)
def test_a_gate_runs_and_its_verdict_is_kept(
    workflow: str,
    job_id: str,
    command: str,
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """Each pinned gate is a step that runs it and whose failure stops the job.

    The job's own ``continue-on-error`` is checked alongside the step's: a job
    carrying it discards every verdict inside while leaving each command
    untouched, so a gate contract reading only the step would pass against a
    lane that cannot fail.
    """
    job = job_by_coordinate(workflow, job_id, documents)
    assert not job.has_continue_on_error, (
        f"{workflow}:{job_id} carries continue-on-error, so no gate in it can fail"
    )
    assert not job.has_condition, (
        f"{workflow}:{job_id} carries an if, which can switch every gate in it off"
    )
    gating = _gating_steps(job, command)
    assert len(gating) == 1, (
        f"{workflow}:{job_id} has {len(gating)} steps whose whole run body is "
        f"{command!r} and which carry neither if nor continue-on-error; "
        f"expected exactly one. Steps present: "
        f"{[step.run for step in job.steps if step.run is not None]}"
    )
