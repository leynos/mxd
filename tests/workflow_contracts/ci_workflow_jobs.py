"""Job and step records parsed from this repository's CI workflows.

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

Parsing primitives live in :mod:`ci_workflow_reader`. Nothing here touches the
filesystem or the YAML parser: every function takes parsed documents, which a
caller obtains from :func:`ci_workflow_reader.repository_documents` or builds
itself. A query that loaded the repository when its argument was omitted would
put a second, invisible route through the boundary the reader owns.
"""

from __future__ import annotations

import dataclasses as dc
import re
import typing as typ

from ci_workflow_reader import (
    WorkflowShapeError,
    optional_text,
    sub_mapping,
)

if typ.TYPE_CHECKING:
    import collections.abc as cabc


@dc.dataclass(frozen=True, slots=True)
class StepRecord:
    """One parsed workflow step.

    Attributes
    ----------
    index
        The step's position within its job, counted from zero. Contracts use
        it to assert ordering, such as a cache restore preceding its save.
    name
        The step's ``name``, or ``None`` when it declares none.
    identifier
        The step's ``id``, or ``None``. A cache save's guard refers to its
        restore step by this identifier.
    uses
        The step's ``uses`` coordinate and ref, or ``None`` for a ``run`` step.
    run
        The step's whole ``run`` body, or ``None`` for a ``uses`` step. It is
        kept whole because whole-value equality is the only reading a wrapped
        command cannot satisfy.
    has_condition
        Whether the step declares an ``if`` key at all. Presence, not value: a
        reading that coerced ``if`` to text would report the YAML boolean
        ``false`` as an empty string and call the step unconditional.
    has_continue_on_error
        Whether the step declares ``continue-on-error``, which discards the
        step's verdict rather than skipping it.
    condition
        The declared condition rendered as text, or ``None`` when absent. Used
        where a contract pins a legitimate condition verbatim.
    with_values
        The step's ``with`` mapping, empty when it declares none.
    """

    index: int
    name: str | None
    identifier: str | None
    uses: str | None
    run: str | None
    has_condition: bool
    has_continue_on_error: bool
    condition: str | None
    with_values: cabc.Mapping[str, object]


# A conditional `runs-on`. GitHub renders one label from a guard and two
# literal arms; the reader keeps the guard as well as the arms, because the
# arms alone cannot tell a fork fallback from an event-keyed placement.
RUNS_ON_EXPRESSION: typ.Final = re.compile(
    r"^\$\{\{\s*(?P<guard>.+?)\s*&&\s*'(?P<when_true>[^']+)'"
    r"\s*\|\|\s*'(?P<when_false>[^']+)'\s*\}\}$"
)

# A placement taken from a workflow input. A reusable workflow that names its
# runner this way has no label of its own: the caller supplies it, so the
# placement is pinned in the caller's `with` block and this record says only
# which input carries it.
RUNS_ON_INPUT: typ.Final = re.compile(
    r"^\$\{\{\s*inputs\.(?P<input>[A-Za-z_][A-Za-z0-9_-]*)\s*\}\}$"
)


@dc.dataclass(frozen=True, slots=True)
class RunnerPlacement:
    """Where a job runs, as its ``runs-on`` declares it.

    Attributes
    ----------
    guard
        The expression deciding between the labels, or ``None`` when the
        placement is unconditional. Kept as text: a contract that read only
        the labels could not tell a fork fallback from an event-keyed
        placement, and those are different decisions.
    declaration
        The value as YAML handed it over, for a placement written as one
        string, and ``None`` for a list of labels. Kept because the parse
        deliberately tolerates whitespace, including a line break a folded
        scalar failed to fold away, and a contract may want to refuse what
        the reader accepts.
    labels
        Every label the job can run on. For a literal placement these are the
        labels in declaration order; for an expression they are the arms in
        expression order, the guarded one first, so a contract can reject a
        pair that was swapped. Empty when the placement comes from an input,
        because this workflow names no label at all.
    from_input
        The workflow input the label is taken from, or ``None`` when the
        placement names its labels here. A job placed this way is pinned in
        its caller, not here, and a contract that read only this file would
        pass while the caller sent the job to any runner it liked.
    """

    guard: str | None
    labels: tuple[str, ...]
    declaration: str | None
    from_input: str | None = None


@dc.dataclass(frozen=True, slots=True)
class JobRecord:
    """One parsed workflow job, with its runner placement and steps.

    Attributes
    ----------
    workflow
        The workflow file name the job is declared in.
    job_id
        The job's identifier within that workflow.
    placement
        Where the job runs, parsed from ``runs-on``: the guard, when the
        placement is conditional, and the labels it chooses between.
    timeout_minutes
        The declared ceiling, left as the parsed value rather than narrowed,
        so a contract can tell an absent ceiling from a non-integer one.
    has_condition
        Whether the job declares an ``if`` key at all. A condition on the job
        disables every gate inside it while leaving each command untouched.
    has_continue_on_error
        Whether the job declares ``continue-on-error``.
    steps
        The job's steps, in declaration order.
    """

    workflow: str
    job_id: str
    placement: RunnerPlacement
    timeout_minutes: object
    has_condition: bool
    has_continue_on_error: bool
    steps: tuple[StepRecord, ...]

    @property
    def coordinate(self) -> tuple[str, str]:
        """The ``(workflow file, job id)`` pair naming this job."""
        return (self.workflow, self.job_id)

    @property
    def runner_labels(self) -> tuple[str, ...]:
        """Every label this job can run on.

        Returns
        -------
        tuple[str, ...]
            The placement's labels. Contracts that judge a label set, such as
            the ceiling and foreign-family rules, read this and so treat a
            conditional placement as the set of runners it can reach.
        """
        return self.placement.labels


@dc.dataclass(frozen=True, slots=True)
class CallRecord:
    """One job that calls a reusable workflow rather than declaring steps.

    Such a job has no ``runs-on`` of its own. The called workflow decides where
    it runs, and where that workflow takes its label from an input, the caller
    decides. Keeping the caller's inputs here is what lets a contract pin the
    label at the only place it is written.

    Attributes
    ----------
    workflow
        The calling workflow's file name.
    job_id
        The job's identifier within that workflow.
    calls
        The ``uses`` value: a path for a workflow in this repository, or an
        owner, repository and ref for one outside it.
    inputs
        The caller's ``with`` mapping, empty when it passes none. Values are
        kept as parsed, so a literal label and an expression are
        distinguishable.
    has_condition
        Whether the call declares an ``if`` key at all. Presence, not value,
        for the reason given on :class:`StepRecord`.
    """

    workflow: str
    job_id: str
    calls: str
    inputs: cabc.Mapping[str, object]
    has_condition: bool

    @property
    def coordinate(self) -> tuple[str, str]:
        """The ``(workflow file, job id)`` pair naming this call."""
        return (self.workflow, self.job_id)


def _expression_placement(value: str, coordinate: str) -> RunnerPlacement:
    """Read a conditional ``runs-on`` expression.

    Parameters
    ----------
    value
        The declared text. A wrapped expression reads the same as a
        single-line one: the whitespace between the guard and its arms is
        matched as whitespace, whether YAML left a space or a line break
        there.
    coordinate
        ``workflow:job`` text used in any error raised.

    Returns
    -------
    RunnerPlacement
        The guard and the two arms, guarded arm first.

    Raises
    ------
    WorkflowShapeError
        When the expression is not the guard-and-two-literal-arms shape this
        estate uses. Reading an unmodelled expression as a literal label
        would leave a lane placed by something no contract can see.
    """
    matched = RUNS_ON_EXPRESSION.match(value.strip())
    if matched is None:
        message = (
            f"{coordinate}: runs-on expression {value!r} is not a guard with "
            "two literal labels"
        )
        raise WorkflowShapeError(message)
    return RunnerPlacement(
        guard=matched["guard"],
        labels=(matched["when_true"], matched["when_false"]),
        declaration=value,
    )


def _runner_placement(
    job: cabc.Mapping[str, object], coordinate: str
) -> RunnerPlacement:
    """Read where a job runs.

    Parameters
    ----------
    job
        A parsed job mapping.
    coordinate
        ``workflow:job`` text used in any error raised.

    Returns
    -------
    RunnerPlacement
        The parsed placement.

    Raises
    ------
    WorkflowShapeError
        When ``runs-on`` is neither a string, nor a list of strings, nor one
        of the two expression shapes above. A runner group is deliberately not
        modelled.
    """
    match job.get("runs-on"):
        case str() as value if matched := RUNS_ON_INPUT.match(value.strip()):
            return RunnerPlacement(
                guard=None,
                labels=(),
                declaration=value,
                from_input=matched["input"],
            )
        case str() as value if "${{" in value:
            return _expression_placement(value, coordinate)
        case str() as label:
            return RunnerPlacement(guard=None, labels=(label,), declaration=label)
        case list() as labels if all(isinstance(entry, str) for entry in labels):
            return RunnerPlacement(
                guard=None,
                labels=tuple(typ.cast("list[str]", labels)),
                declaration=None,
            )
        case other:
            message = (
                f"{coordinate}: runs-on must be a string or a list of strings, "
                f"got {other!r}"
            )
            raise WorkflowShapeError(message)


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
    )


def _job_record(
    workflow_name: str, job_id: str, job: cabc.Mapping[str, object]
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
        placement=_runner_placement(job, coordinate),
        timeout_minutes=job.get("timeout-minutes"),
        has_condition="if" in job,
        has_continue_on_error="continue-on-error" in job,
        steps=tuple(
            _step_record(index, sub_mapping(step, coordinate), coordinate)
            for index, step in enumerate(typ.cast("list[object]", steps))
        ),
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
        for job_id, job in _job_mapping(name, workflow).items():
            narrowed = sub_mapping(job, f"{name}:{job_id}")
            if "uses" in narrowed:
                continue
            records.append(_job_record(name, str(job_id), narrowed))
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
        When the coordinate does not name exactly one job.
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


def call_by_coordinate(
    workflow: str,
    job_id: str,
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> CallRecord:
    """Select one reusable-workflow call by its workflow file and identifier.

    Parameters
    ----------
    workflow
        The calling workflow's file name.
    job_id
        The job identifier within that workflow.
    documents
        Parsed workflows to search. Required, for the reason given on
        :func:`job_records`.

    Returns
    -------
    CallRecord
        The matching call.

    Raises
    ------
    WorkflowShapeError
        When the coordinate does not name exactly one call.
    """
    matches = [
        record
        for record in call_records(documents)
        if record.coordinate == (workflow, job_id)
    ]
    if len(matches) != 1:
        message = f"expected exactly one {workflow}:{job_id} call, found {len(matches)}"
        raise WorkflowShapeError(message)
    return matches[0]
