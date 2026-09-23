"""The records a workflow contract reads, and the distinctions they keep.

A record keeps what a contract depends on and discards nothing it might need.
Three choices here are deliberate and each exists because the obvious reading
loses a case.

A condition is recorded as key presence rather than as a value, because a
reader coercing ``if`` to text would report the YAML boolean ``false`` as an
empty string and call the step unconditional. A step's ``run`` body is kept
whole, because whole-value equality is the only reading a wrapped command
cannot satisfy. And a job calling a reusable workflow is a :class:`CallRecord`
rather than a refusal, because this repository has three such jobs and one of
them is where a lane's placement is actually decided.

The readers that build these live in :mod:`ci_workflow_jobs`, the placement
each job carries is read in :mod:`ci_workflow_placement`, and the parsing
primitives are in :mod:`ci_workflow_reader`. Nothing here touches the
filesystem or the YAML parser.
"""

from __future__ import annotations

import dataclasses as dc
import typing as typ

from ci_workflow_placement import RunnerPlacement

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
    execution_overrides
        Which of ``shell`` and ``working-directory`` the step declares. Either
        changes what its ``run`` body means without changing the body.
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
    execution_overrides: tuple[str, ...] = ()


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
    run_defaults
        Each ``defaults.run`` setting that reaches the job's steps, as
        ``scope:key`` with scope ``workflow`` or ``job``. A default changes the
        shell or directory of every ``run`` body it reaches.
    """

    workflow: str
    job_id: str
    placement: RunnerPlacement
    timeout_minutes: object
    has_condition: bool
    has_continue_on_error: bool
    steps: tuple[StepRecord, ...]
    run_defaults: tuple[str, ...] = ()

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
