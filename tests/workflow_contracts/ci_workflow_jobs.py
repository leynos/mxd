"""Reading this repository's CI workflows into job, step and call records.

A record keeps the distinctions a contract depends on. A condition is recorded
as key presence rather than as a value, because a reader that coerced ``if`` to
text would report the YAML boolean ``false`` as an empty string and call the
step unconditional. A step's ``run`` body is kept whole, because whole-value
equality is the only reading a wrapped command cannot satisfy.

This repository calls reusable workflows, so a call is recorded rather than
refused: :class:`CallRecord` keeps the called workflow and the caller's
``with`` mapping. One lane's placement lives there and nowhere else, and a
contract reading the callee's own ``runs-on`` would pass while the caller sent
the job to any runner it liked.

Parsing primitives live in :mod:`ci_workflow_reader` and where a job runs is
read in :mod:`ci_workflow_placement`. Nothing here touches the
filesystem or the YAML parser: every function takes parsed documents, which a
caller obtains from :func:`ci_workflow_reader.repository_documents` or builds
itself. A query that loaded the repository when its argument was omitted would
put a second, invisible route through the boundary the reader owns.
"""

from __future__ import annotations

import typing as typ

from ci_workflow_placement import runner_placement
from ci_workflow_records import CallRecord, JobRecord, StepRecord
from ci_workflow_reader import (
    WorkflowShapeError,
    optional_text,
    sub_mapping,
)

if typ.TYPE_CHECKING:
    import collections.abc as cabc

# The settings that change how a ``run`` body executes without changing it.
EXECUTION_SETTINGS: typ.Final = ("shell", "working-directory")


def _run_defaults(scope: str, owner: cabc.Mapping[str, object]) -> tuple[str, ...]:
    """Name each execution setting a ``defaults.run`` block declares.

    Parameters
    ----------
    scope
        ``workflow`` or ``job``, prefixed to each name.
    owner
        The workflow or job mapping that may carry ``defaults``.

    Returns
    -------
    tuple[str, ...]
        ``scope:key`` for each setting present, empty when there is none.
    """
    defaults = owner.get("defaults")
    run = defaults.get("run") if isinstance(defaults, dict) else None
    if not isinstance(run, dict):
        return ()
    return tuple(f"{scope}:{key}" for key in EXECUTION_SETTINGS if key in run)


def _step_record(
    index: int, step: cabc.Mapping[str, object], coordinate: str
) -> StepRecord:
    """Build one step record.

    Parameters
    ----------
    index
        The step's position within its job.
    step
        The parsed step mapping.
    coordinate
        ``workflow:job`` text used in any error raised.

    Returns
    -------
    StepRecord
        The parsed step.
    """
    condition = step.get("if")
    return StepRecord(
        index=index,
        name=optional_text(step.get("name"), coordinate),
        identifier=optional_text(step.get("id"), coordinate),
        uses=optional_text(step.get("uses"), coordinate),
        run=optional_text(step.get("run"), coordinate),
        has_condition="if" in step,
        has_continue_on_error="continue-on-error" in step,
        condition=None if condition is None else str(condition),
        with_values=sub_mapping(step.get("with"), coordinate),
        execution_overrides=tuple(key for key in EXECUTION_SETTINGS if key in step),
    )


def _job_record(
    workflow_name: str,
    job_id: str,
    job: cabc.Mapping[str, object],
    workflow_defaults: tuple[str, ...] = (),
) -> JobRecord:
    """Build one job record.

    Parameters
    ----------
    workflow_name
        The workflow file the job belongs to.
    job_id
        The job's identifier within that workflow.
    job
        The parsed job mapping.
    workflow_defaults
        The workflow-level ``defaults.run`` settings, which reach every job.

    Returns
    -------
    JobRecord
        The parsed job.

    Raises
    ------
    WorkflowShapeError
        When the job calls a reusable workflow, whose runner placement lives in
        the called workflow, or when it declares no step list.
    """
    coordinate = f"{workflow_name}:{job_id}"
    steps = job.get("steps")
    if not isinstance(steps, list):
        message = f"{coordinate}: job must declare steps"
        raise WorkflowShapeError(message)
    return JobRecord(
        workflow=workflow_name,
        job_id=job_id,
        placement=runner_placement(job, coordinate),
        timeout_minutes=job.get("timeout-minutes"),
        has_condition="if" in job,
        has_continue_on_error="continue-on-error" in job,
        steps=tuple(
            _step_record(index, sub_mapping(step, coordinate), coordinate)
            for index, step in enumerate(typ.cast("list[object]", steps))
        ),
        run_defaults=workflow_defaults + _run_defaults("job", job),
    )


def _call_record(
    workflow_name: str, job_id: str, job: cabc.Mapping[str, object]
) -> CallRecord:
    """Build one reusable-workflow call record.

    Parameters
    ----------
    workflow_name
        The calling workflow's file name.
    job_id
        The job's identifier within that workflow.
    job
        The parsed job mapping, which declares ``uses``.

    Returns
    -------
    CallRecord
        The call, with the caller's inputs.

    Raises
    ------
    WorkflowShapeError
        When ``uses`` is not a string.
    """
    coordinate = f"{workflow_name}:{job_id}"
    called = optional_text(job.get("uses"), coordinate)
    if called is None:
        message = f"{coordinate}: a call must name the workflow it uses"
        raise WorkflowShapeError(message)
    return CallRecord(
        workflow=workflow_name,
        job_id=job_id,
        calls=called,
        inputs=sub_mapping(job.get("with"), coordinate),
        has_condition="if" in job,
    )


def _job_mapping(
    name: str, workflow: cabc.Mapping[str, object]
) -> cabc.Mapping[str, object]:
    """Read one workflow's job mapping.

    Parameters
    ----------
    name
        The workflow file name, used in any error raised.
    workflow
        The parsed workflow document.

    Returns
    -------
    cabc.Mapping[str, object]
        The workflow's jobs.

    Raises
    ------
    WorkflowShapeError
        When the workflow declares no job mapping.
    """
    jobs = workflow.get("jobs")
    if not isinstance(jobs, dict):
        message = f"{name} must declare jobs"
        raise WorkflowShapeError(message)
    return typ.cast("cabc.Mapping[str, object]", jobs)


def job_records(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> tuple[JobRecord, ...]:
    """Parse every step-declaring job in a set of workflow documents.

    A job that calls a reusable workflow declares no steps and no placement of
    its own, so it is not a job record. :func:`call_records` reads those. Every
    job in the tree is one or the other, and the placement contract pins both
    sets in both directions, so a job cannot fall between them unnoticed.

    Parameters
    ----------
    documents
        Parsed workflows keyed by file name. Required: this function reads
        nothing of its own, so a contract passes
        :func:`ci_workflow_reader.repository_documents` and a unit test passes
        the shape it wants to drive.

    Returns
    -------
    tuple[JobRecord, ...]
        Every step-declaring job, in file order and then declaration order.

    Raises
    ------
    WorkflowShapeError
        When a workflow declares no job mapping, or a job that is not a call
        declares no step list.
    """
    records: list[JobRecord] = []
    for name, workflow in documents.items():
        workflow_defaults = _run_defaults("workflow", workflow)
        for job_id, job in _job_mapping(name, workflow).items():
            narrowed = sub_mapping(job, f"{name}:{job_id}")
            if "uses" in narrowed:
                continue
            records.append(_job_record(name, str(job_id), narrowed, workflow_defaults))
    return tuple(records)


def call_records(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> tuple[CallRecord, ...]:
    """Parse every reusable-workflow call in a set of workflow documents.

    Parameters
    ----------
    documents
        Parsed workflows keyed by file name. Required, for the reason given on
        :func:`job_records`.

    Returns
    -------
    tuple[CallRecord, ...]
        Every call, in file order and then declaration order.

    Raises
    ------
    WorkflowShapeError
        When a workflow declares no job mapping, or a call does not name the
        workflow it uses.
    """
    records: list[CallRecord] = []
    for name, workflow in documents.items():
        for job_id, job in _job_mapping(name, workflow).items():
            narrowed = sub_mapping(job, f"{name}:{job_id}")
            if "uses" not in narrowed:
                continue
            records.append(_call_record(name, str(job_id), narrowed))
    return tuple(records)


def job_by_coordinate(
    workflow: str,
    job_id: str,
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> JobRecord:
    """Select one step-declaring job by its workflow file and identifier.

    There is no matching selector for calls. The one contract that needs a
    call by coordinate reads it from :func:`call_records` itself, because a
    second selector differing only in the sequence it searches and the noun it
    names is duplication rather than symmetry.

    Parameters
    ----------
    workflow
        The workflow file name.
    job_id
        The job identifier within that workflow.
    documents
        Parsed workflows to search. Required, for the reason given on
        :func:`job_records`.

    Returns
    -------
    JobRecord
        The matching job.

    Raises
    ------
    WorkflowShapeError
        When the coordinate does not name exactly one job. Zero and two are
        the same failure to a caller expecting one, and both mean a pin and
        the tree have drifted apart.
    """
    matches = [
        record
        for record in job_records(documents)
        if record.coordinate == (workflow, job_id)
    ]
    if len(matches) != 1:
        message = f"expected exactly one {workflow}:{job_id} job, found {len(matches)}"
        raise WorkflowShapeError(message)
    return matches[0]
