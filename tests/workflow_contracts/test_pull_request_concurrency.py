"""Every workflow a pull request starts cancels its own superseded runs.

Every push to a pull request starts a fresh run of each gate, and the run
already in flight is answering a question about a commit nobody will merge.
Left alone it holds a runner until it finishes, so the branch pays twice for
one answer. A concurrency group keyed on the pull request lets the newer run
cancel the older one.

The block is compared by whole-value equality, because each half fails in a
way that still reads as a concurrency control:

- The group keys on the pull request number and falls back to the run id. A
  fallback of ``github.ref`` puts every push to `main`, every schedule and
  every dispatch of a workflow in one group, where a third trigger replaces a
  pending second run that was meant to complete. A run-unique value anywhere
  else, or a constant group, either cancels nothing or lets one pull request
  cancel another's gates.
- Cancellation is conditioned on the event. A literal
  ``cancel-in-progress: true`` reads as the stricter setting and is a
  regression for any event that shares a group.

Only ``pull_request`` is in scope. A ``pull_request_target`` workflow here
automates pull-request housekeeping rather than building, and cancelling an
auto-merge mid-flight is a hazard with no minutes to win.

The judgement is :func:`concurrency_defects`, which takes a parsed document,
so the unit cases below drive it with the shapes this repository does not
declare. A contract parametrized over the workflows as they stand would pass
unchanged with the judgement deleted.
"""

from __future__ import annotations

import typing as typ

import pytest
from ci_workflow_reader import repository_documents, triggers

if typ.TYPE_CHECKING:
    import collections.abc as cabc

#: The trigger that puts a workflow in scope. `pull_request_target` is
#: deliberately absent; see the module docstring.
PULL_REQUEST: typ.Final = "pull_request"

#: The whole group expression. The run id is permitted in the fallback
#: position only, where it is reached by every event that is not a pull
#: request.
CONCURRENCY_GROUP: typ.Final = (
    "${{ github.workflow }}-${{ github.event.pull_request.number || github.run_id }}"
)

#: The whole `cancel-in-progress` expression. A YAML `true` is a boolean and
#: never equals this string, which is what makes the literal fail.
CANCEL_IN_PROGRESS: typ.Final = "${{ github.event_name == 'pull_request' }}"

#: Workflows known to start on `pull_request`. Discovery is dynamic, so a new
#: workflow is covered the day it lands, but a discovered list that silently
#: empties turns every parametrized contract into a vacuous pass. This is the
#: floor discovery must still reach.
KNOWN_PULL_REQUEST_WORKFLOWS: typ.Final = frozenset(
    {"ci.yml", "loom-check.yml", "release-dry-run.yml", "tlc-image.yml", "tlc.yml"}
)


def pull_request_workflows(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> tuple[str, ...]:
    """Name every workflow a pull request can start.

    Triggers are read through :func:`ci_workflow_reader.triggers`, which
    refuses a trigger block it does not model rather than letting discovery
    drop the workflow in silence.

    Parameters
    ----------
    documents
        Parsed workflows keyed by file name.

    Returns
    -------
    tuple[str, ...]
        The names of the workflows declaring a `pull_request` trigger, sorted.
    """
    return tuple(
        sorted(name for name, doc in documents.items() if PULL_REQUEST in triggers(doc))
    )


def concurrency_defects(document: cabc.Mapping[str, object]) -> tuple[str, ...]:
    """Describe how a workflow's concurrency block departs from the contract.

    Parameters
    ----------
    document
        One parsed workflow.

    Returns
    -------
    tuple[str, ...]
        One sentence per defect, empty when the block is exactly the
        contracted one. The string shorthand `concurrency: <group>` is a
        defect, because it cannot carry `cancel-in-progress` at all.
    """
    declared = document.get("concurrency")
    if not isinstance(declared, dict):
        return (f"concurrency must be a mapping, not {declared!r}",)
    defects: list[str] = []
    group = declared.get("group")
    if group != CONCURRENCY_GROUP:
        defects.append(f"group is {group!r}, expected {CONCURRENCY_GROUP!r}")
    cancel = declared.get("cancel-in-progress")
    if cancel != CANCEL_IN_PROGRESS:
        defects.append(
            f"cancel-in-progress is {cancel!r}, expected {CANCEL_IN_PROGRESS!r}"
        )
    return tuple(defects)


@pytest.fixture(scope="module")
def documents() -> cabc.Mapping[str, cabc.Mapping[str, object]]:
    """This repository's parsed workflows."""
    return repository_documents()


def test_discovery_still_finds_the_known_pull_request_workflows(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """Discovery reaches its floor, so the contract below is not vacuous."""
    missing = sorted(
        KNOWN_PULL_REQUEST_WORKFLOWS - set(pull_request_workflows(documents))
    )
    assert not missing, (
        f"these workflows start on pull_request but discovery missed them: "
        f"{', '.join(missing)}; the concurrency contract would pass without "
        "asserting anything about them"
    )


def test_every_pull_request_workflow_cancels_its_superseded_runs(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """Each workflow a pull request starts carries the contracted block."""
    defects = {
        name: concurrency_defects(documents[name])
        for name in pull_request_workflows(documents)
    }
    assert not {name: found for name, found in defects.items() if found}, defects


@pytest.mark.parametrize(
    ("concurrency", "expected"),
    [
        pytest.param(None, "must be a mapping", id="block-absent"),
        pytest.param("ci-${{ github.ref }}", "must be a mapping", id="shorthand"),
        pytest.param(
            {
                "group": CONCURRENCY_GROUP.replace("github.run_id", "github.ref"),
                "cancel-in-progress": CANCEL_IN_PROGRESS,
            },
            "group is",
            id="ref-fallback",
        ),
        pytest.param(
            {
                "group": "${{ github.workflow }}-${{ github.run_id }}",
                "cancel-in-progress": CANCEL_IN_PROGRESS,
            },
            "group is",
            id="run-id-only",
        ),
        pytest.param(
            {"group": "ci", "cancel-in-progress": CANCEL_IN_PROGRESS},
            "group is",
            id="constant-group",
        ),
        pytest.param(
            {"group": CONCURRENCY_GROUP, "cancel-in-progress": True},
            "cancel-in-progress is True",
            id="literal-true",
        ),
        pytest.param(
            {"group": CONCURRENCY_GROUP},
            "cancel-in-progress is None",
            id="cancel-absent",
        ),
    ],
)
def test_a_departing_block_is_refused(concurrency: object, expected: str) -> None:
    """Each weaker block is reported, not accepted."""
    document = {True: {PULL_REQUEST: None}, "concurrency": concurrency}
    defects = concurrency_defects(document)
    assert any(expected in defect for defect in defects), defects


def test_the_contracted_block_is_accepted() -> None:
    """The exact block yields no defect, so the refusals above discriminate."""
    document = {
        "concurrency": {
            "group": CONCURRENCY_GROUP,
            "cancel-in-progress": CANCEL_IN_PROGRESS,
        }
    }
    assert concurrency_defects(document) == ()


def test_a_pull_request_target_workflow_is_out_of_scope() -> None:
    """Discovery reads `pull_request` only, not `pull_request_target`."""
    documents = {
        "automerge.yml": {True: {"pull_request_target": None}},
        "ci.yml": {True: {PULL_REQUEST: {"branches": ["main"]}}},
    }
    assert pull_request_workflows(documents) == ("ci.yml",)
