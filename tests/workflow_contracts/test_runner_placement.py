"""Where every CI job runs, and how long it may run for.

Two questions, asserted together because they share a subject. A lane's runner
decides what it costs and whether a fork's pull request can obtain it; a lane's
ceiling decides whether a job that stops making progress is abandoned or spends
six hours doing it.

Both are pinned by coordinate and compared in both directions, so a new lane
cannot appear unpinned and a pin cannot outlive the lane it named. A subset
assertion in either direction passes against a tree that has lost half its
jobs.

One lane is pinned in its caller. ``build-and-package.yml``'s ``build`` takes
its label from an input, so this file is not where its placement is decided;
``release.yml``'s ``build-linux`` passes the label and is asserted there. A
contract reading the callee's own ``runs-on`` would pass while the caller sent
the job to any runner it liked, which is the defect worth catching.
"""

from __future__ import annotations

import re
import typing as typ

import pytest
from ci_workflow_jobs import (
    call_records,
    job_by_coordinate,
    job_records,
)
from ci_workflow_reader import repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

# Every job that declares its own steps, with the label it runs on. Nothing
# here is paid for: the repository has not migrated any lane to a paid
# provider, so a label outside this set is either a migration nobody pinned or
# a typo GitHub would queue forever.
PINNED_PLACEMENTS: typ.Final[cabc.Mapping[tuple[str, str], tuple[str, ...]]] = {
    ("audit.yml", "audit"): ("ubuntu-latest",),
    ("ci.yml", "docs-tooling"): ("ubuntu-latest",),
    ("ci.yml", "build-test"): ("ubuntu-latest",),
    ("ci.yml", "validator-sqlite"): ("ubuntu-latest",),
    ("ci.yml", "coverage"): ("ubuntu-latest",),
    ("coverage-main.yml", "coverage-upload"): ("ubuntu-latest",),
    ("fuzz.yml", "fuzz"): ("ubuntu-latest",),
    ("release.yml", "metadata"): ("ubuntu-latest",),
    ("release.yml", "release"): ("ubuntu-latest",),
    ("tlc-image.yml", "build-and-push"): ("ubuntu-latest",),
    ("tlc.yml", "tlc"): ("ubuntu-latest",),
}

# The one lane whose placement lives in an input rather than in a label.
PLACEMENT_FROM_INPUT: typ.Final[cabc.Mapping[tuple[str, str], str]] = {
    ("build-and-package.yml", "build"): "runner",
}

# Every reusable-workflow call, with the workflow it calls. The identity is
# pinned; the ref deliberately is not. A Dependabot bump moving a pin forward
# is a decision this repository already makes elsewhere, and a contract
# naming the SHA would redden every such pull request and teach people to edit
# the contract to make a bump pass, which is the habit worth avoiding.
#
# What is asserted instead is that an external call is pinned to a commit at
# all. A ref that is a branch or a tag is the substantive defect: it moves
# under the workflow without any pull request at all.
PINNED_CALLS: typ.Final[cabc.Mapping[tuple[str, str], str]] = {
    ("dependabot-automerge.yml", "automerge"): (
        "leynos/shared-actions/.github/workflows/dependabot-automerge.yml"
    ),
    ("release-dry-run.yml", "release"): "./.github/workflows/release.yml",
    ("release.yml", "build-linux"): "./.github/workflows/build-and-package.yml",
}

# A 40-character lower-case hexadecimal commit, which is the only ref that
# cannot move under the workflow that names it.
COMMIT_REF: typ.Final = re.compile(r"^[0-9a-f]{40}$")
# The two spellings GitHub documents for a call into this repository; either
# names the tree under review and carries no ref to pin.
LOCAL_CALL_PREFIXES: typ.Final = ("./", "$/")


def is_external_call(calls: str) -> bool:
    """Whether a ``uses`` value names another repository's workflow."""
    return not calls.strip().startswith(LOCAL_CALL_PREFIXES)


# Ceilings, in minutes. Sized as the estate sizes them: about twice the worst
# observed successful duration plus a quarter of an hour of reporting margin.
# A lane with no ceiling inherits GitHub's six-hour default, which is not a
# bound on anything: it is the point at which a wedged job stops costing money.
#
# `tlc-image` is sized on the image build rather than on the median. It skips
# in 24 to 29 s on nearly every run because the Dockerfile rarely changes, and
# a ceiling sized on that would cancel the only run that matters.
#
# The release path cannot be measured while `release-dry-run` fails at startup
# (#549), so `metadata`, `release` and `build` carry placeholders generous
# enough not to cancel a real run and tight enough to catch a wedged one.
PINNED_CEILINGS: typ.Final[cabc.Mapping[tuple[str, str], int]] = {
    ("audit.yml", "audit"): 20,
    ("build-and-package.yml", "build"): 45,
    ("ci.yml", "docs-tooling"): 25,
    ("ci.yml", "build-test"): 45,
    ("ci.yml", "validator-sqlite"): 27,
    ("ci.yml", "coverage"): 45,
    ("coverage-main.yml", "coverage-upload"): 50,
    ("fuzz.yml", "fuzz"): 360,
    ("release.yml", "metadata"): 30,
    ("release.yml", "release"): 30,
    ("tlc-image.yml", "build-and-push"): 120,
    ("tlc.yml", "tlc"): 45,
}


@pytest.fixture(scope="module")
def documents() -> cabc.Mapping[str, cabc.Mapping[str, object]]:
    """This repository's parsed workflows."""
    return repository_documents()


def test_every_pinned_job_exists(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """A pin naming no job is a pin nothing can fail.

    This is the direction that catches a rename: the lane still runs, the pin
    still reads, and neither says anything about the other.
    """
    declared = {record.coordinate for record in job_records(documents)}
    pinned = set(PINNED_PLACEMENTS) | set(PLACEMENT_FROM_INPUT)
    assert pinned <= declared, (
        f"pinned coordinates naming no job: {sorted(pinned - declared)}"
    )


def test_every_job_is_pinned(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """A lane nobody pinned is a lane nobody decided the placement of."""
    declared = {record.coordinate for record in job_records(documents)}
    pinned = set(PINNED_PLACEMENTS) | set(PLACEMENT_FROM_INPUT)
    assert declared <= pinned, (
        f"jobs with no pinned placement: {sorted(declared - pinned)}"
    )


@pytest.mark.parametrize(("coordinate", "labels"), sorted(PINNED_PLACEMENTS.items()))
def test_a_job_runs_where_it_is_pinned(
    coordinate: tuple[str, str],
    labels: tuple[str, ...],
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """Each lane's labels are exactly the pinned ones, in order.

    Order matters for a conditional placement, where the arms read in
    expression order: a pair compared unordered accepts the labels swapped,
    which is a different lane entirely.
    """
    record = job_by_coordinate(*coordinate, documents)
    assert record.runner_labels == labels, (
        f"{coordinate[0]}:{coordinate[1]} runs on {record.runner_labels}, "
        f"pinned as {labels}"
    )


@pytest.mark.parametrize(
    ("coordinate", "input_name"), sorted(PLACEMENT_FROM_INPUT.items())
)
def test_a_called_lane_takes_its_label_from_its_input(
    coordinate: tuple[str, str],
    input_name: str,
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """A reusable workflow names the input carrying its label, not a label."""
    record = job_by_coordinate(*coordinate, documents)
    assert record.placement.from_input == input_name, (
        f"{coordinate[0]}:{coordinate[1]} takes its runner from "
        f"{record.placement.from_input!r}, pinned as {input_name!r}"
    )
    assert record.runner_labels == (), (
        "a lane placed by its caller must name no label of its own"
    )


def test_the_caller_sends_the_package_build_to_a_pinned_runner(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """The one placement decided elsewhere is asserted where it is decided.

    ``build-and-package.yml`` reads its label from ``inputs.runner``, so its
    own file says nothing about where the build lands. ``release.yml``'s
    ``build-linux`` is the only caller, and this is the assertion that the
    label it passes is a literal this contract has read, not an expression
    resolved somewhere no contract looks.
    """
    (call,) = [
        record
        for record in call_records(documents)
        if record.coordinate == ("release.yml", "build-linux")
    ]
    assert call.inputs.get("runner") == "ubuntu-latest", (
        f"release.yml:build-linux sends the package build to "
        f"{call.inputs.get('runner')!r}"
    )


def test_every_call_is_pinned_and_every_pin_is_a_call(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """The set of reusable-workflow calls is exactly the pinned set.

    Compared on the workflow's identity with any ref stripped, so that moving
    a pin forward is not a contract failure while calling somewhere new is.
    """
    declared = {
        record.coordinate: record.calls.split("@", 1)[0]
        for record in call_records(documents)
    }
    assert declared == dict(PINNED_CALLS), (
        f"calls declared {sorted(declared.items())}, "
        f"pinned {sorted(PINNED_CALLS.items())}"
    )


def test_every_call_outside_this_repository_names_a_commit(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """An external call is pinned to a commit, never to a branch or a tag.

    A call to a path in this repository carries no ref and needs none: it is
    the tree under review. A call to another repository is a different matter.
    A branch ref moves under the workflow with no pull request here at all, so
    a lane could change what it runs between two identical trees, and a tag can
    be moved in place.
    """
    external = [
        record for record in call_records(documents) if is_external_call(record.calls)
    ]
    assert external, "expected at least one call outside this repository"
    unpinned = [
        (record.coordinate, record.calls)
        for record in external
        if COMMIT_REF.match(record.calls.partition("@")[2]) is None
    ]
    assert not unpinned, f"calls not pinned to a commit: {unpinned}"


def test_every_job_has_a_pinned_ceiling(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """The set of jobs and the set of ceilings are the same set.

    Asserted in both directions for the reason the placements are: a ceiling
    naming no job bounds nothing, and a job naming no ceiling inherits six
    hours.
    """
    declared = {record.coordinate for record in job_records(documents)}
    assert declared == set(PINNED_CEILINGS), (
        f"jobs without a pinned ceiling: {sorted(declared - set(PINNED_CEILINGS))}; "
        f"ceilings naming no job: {sorted(set(PINNED_CEILINGS) - declared)}"
    )


@pytest.mark.parametrize(("coordinate", "minutes"), sorted(PINNED_CEILINGS.items()))
def test_a_job_declares_the_ceiling_it_is_pinned_to(
    coordinate: tuple[str, str],
    minutes: int,
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """Each lane declares its ceiling, as an integer number of minutes.

    The declared value is compared without narrowing, so a ceiling written as
    a string, which GitHub accepts and which no reader here would notice,
    fails rather than passing as the number it looks like.
    """
    record = job_by_coordinate(*coordinate, documents)
    assert record.timeout_minutes == minutes, (
        f"{coordinate[0]}:{coordinate[1]} declares timeout-minutes "
        f"{record.timeout_minutes!r}, pinned as {minutes}"
    )
    assert isinstance(record.timeout_minutes, int), (
        f"{coordinate[0]}:{coordinate[1]} declares a ceiling that is not an "
        f"integer: {record.timeout_minutes!r}"
    )
