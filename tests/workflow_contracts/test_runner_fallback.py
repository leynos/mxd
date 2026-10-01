"""A conditional placement falls back to hosted for a fork's pull request.

Ubicloud's cache proxy is scoped by ref, and a fork's pull request cannot
obtain an Ubicloud runner at all. Pinning the labels, as
``test_runner_placement`` does, would accept the same pair under another guard,
so this module holds the guard itself.
"""

from __future__ import annotations

import typing as typ

from ci_workflow_jobs import job_by_coordinate
from ci_workflow_reader import repository_documents
from test_runner_placement import PINNED_PLACEMENTS

#: The guard that keeps a fork's pull request off an Ubicloud runner, which it
#: cannot obtain.
FORK_GUARD: typ.Final = "github.event.pull_request.head.repo.fork"


def test_an_ubicloud_lane_falls_back_to_hosted_for_a_fork() -> None:
    """Every conditional placement is guarded by the fork test, hosted arm first.

    Pinning the labels alone would accept the same pair under another guard, for
    example an event name, which sends a fork's pull request to a runner it
    cannot obtain.
    """
    documents = repository_documents()
    for coordinate, labels in sorted(PINNED_PLACEMENTS.items()):
        if len(labels) != 2:
            continue
        record = job_by_coordinate(*coordinate, documents)
        assert record.placement.guard == FORK_GUARD, (
            f"{coordinate[0]}:{coordinate[1]} is guarded by "
            f"{record.placement.guard!r}, not the fork test"
        )
        assert labels[0] == "ubuntu-latest", (
            f"{coordinate[0]}:{coordinate[1]} falls back to {labels[0]!r}"
        )
