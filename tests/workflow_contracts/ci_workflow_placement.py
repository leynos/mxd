"""Where a workflow job runs, read from its ``runs-on`` declaration.

A placement decides what a lane costs, whether a fork's pull request can obtain
a runner for it, and which provider is billed. Four shapes appear in this
estate and the reader models three of them deliberately: a bare label, a list
of labels, and a conditional expression choosing between two literal arms. A
fourth, a placement taken from a workflow input, names no label here at all:
the caller supplies it, so the record says only which input carries it. A
runner group, and any expression outside those shapes, raise
:class:`ci_workflow_reader.WorkflowShapeError`.

Refusing is the whole design. A reader that recorded an unmodelled shape as a
literal would leave a lane placed by something no contract can see, and every
placement contract would then pass or fail on a label GitHub never evaluates.
A list is the subtle case: GitHub permits a variable among its entries, so an
entry is not self-evidently a label.

Parsing primitives live in :mod:`ci_workflow_reader`; the job, step and call
records that carry a placement live in :mod:`ci_workflow_jobs`. Nothing here
touches the filesystem or the YAML parser.
"""

from __future__ import annotations

import dataclasses as dc
import re
import typing as typ

from ci_workflow_reader import WorkflowShapeError

if typ.TYPE_CHECKING:
    import collections.abc as cabc


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


def _list_placement(labels: list[str], coordinate: str) -> RunnerPlacement:
    """Read a ``runs-on`` written as a list of labels.

    GitHub permits an expression among the entries, so a list is not
    self-evidently a set of literal labels. One recorded as a literal would
    read to every contract as a runner named ``${{ inputs.chosen-os }}``,
    which no job can be placed on and which hides the runners the expression
    can actually select. The list form is refused rather than guessed at, for
    the same reason a runner group is.

    Parameters
    ----------
    labels
        The declared entries, already narrowed to strings.
    coordinate
        ``workflow:job`` text used in any error raised.

    Returns
    -------
    RunnerPlacement
        The labels in declaration order.

    Raises
    ------
    WorkflowShapeError
        When an entry contains an expression, or when the list is empty.
    """
    if not labels:
        message = f"{coordinate}: runs-on must name at least one label"
        raise WorkflowShapeError(message)
    expressions = [label for label in labels if "${{" in label]
    if expressions:
        message = (
            f"{coordinate}: runs-on list entries {expressions!r} are "
            "expressions, and the runners they select are not modelled here"
        )
        raise WorkflowShapeError(message)
    return RunnerPlacement(guard=None, labels=tuple(labels), declaration=None)


def runner_placement(
    job: cabc.Mapping[str, object], coordinate: str
) -> RunnerPlacement:
    """Read where a job runs.

    The one public reader here. :mod:`ci_workflow_jobs` calls it once per job
    and keeps the result on the record; nothing else needs the shape readers
    behind it.

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
        of the two expression shapes above, or when a list entry is an
        expression. A runner group is deliberately not modelled.
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
            return _list_placement(typ.cast("list[str]", labels), coordinate)
        case other:
            message = (
                f"{coordinate}: runs-on must be a string or a list of strings, "
                f"got {other!r}"
            )
            raise WorkflowShapeError(message)
