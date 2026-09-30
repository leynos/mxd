"""A called workflow may not request more token permission than its caller grants.

GitHub validates a reusable-workflow call before it starts any job. When a job
of the callee asks for a scope the calling job does not grant, the whole run
fails at startup: no job is listed and no check appears on the pull request.
`release-dry-run.yml` did exactly that. It capped the token at
``contents: read`` while `release.yml`'s `release` job asks for
``contents: write``, so the dry run never started on any pull request, and
nothing in the pull request said so.

The judgement is :func:`permission_shortfalls`, over parsed documents, so the
unit cases drive it with shapes this repository does not declare. The
repository case then holds every local call to it. A contract parametrized over
the workflows as they stand would pass unchanged with the judgement deleted.
"""

from __future__ import annotations

import typing as typ

import pytest
from ci_workflow_reader import repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc


#: Access levels in ascending order; a scope not named grants none.
LEVELS: typ.Final = {"none": 0, "read": 1, "write": 2}
LOCAL_CALL_PREFIXES: typ.Final = ("./.github/workflows/", "$/.github/workflows/")


def _scopes(value: object) -> dict[str, int] | None:
    """Return the scopes a `permissions` value grants, or None when unset.

    `read-all` and `write-all` are read as every scope at that level, which is
    stricter than the truth for the caller and so cannot hide a shortfall.
    """
    match value:
        case None:
            return None
        case "read-all" | "write-all":
            level = LEVELS[str(value).split("-")[0]]
            return {"*": level}
        case dict():
            return {str(k): LEVELS.get(str(v), 0) for k, v in value.items()}
        case _:
            return {}


def _granted(scopes: dict[str, int], name: str) -> int:
    """Return the level a set of scopes grants for one scope."""
    return scopes.get(name, scopes.get("*", 0))


def _job_scopes(
    document: cabc.Mapping[str, object], job: cabc.Mapping[str, object]
) -> dict[str, int] | None:
    """Return a job's effective scopes: its own, else the workflow's."""
    own = _scopes(job.get("permissions"))
    return own if own is not None else _scopes(document.get("permissions"))


def _jobs(document: cabc.Mapping[str, object]) -> dict[str, cabc.Mapping[str, object]]:
    """Return a workflow's jobs that are mappings."""
    jobs = document.get("jobs")
    if not isinstance(jobs, dict):
        return {}
    return {str(k): v for k, v in jobs.items() if isinstance(v, dict)}


def _local_name(uses: object) -> str | None:
    """Return the file name of a local reusable workflow reference, else None."""
    if not isinstance(uses, str):
        return None
    for prefix in LOCAL_CALL_PREFIXES:
        if uses.startswith(prefix) and "@" not in uses:
            return uses.removeprefix(prefix)
    return None


def permission_shortfalls(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> list[str]:
    """Return each scope a callee job asks for that its calling job does not grant.

    A caller that declares no permissions at all gets the repository default,
    which this reader cannot know, so it is not judged.

    Parameters
    ----------
    documents
        Every parsed workflow, keyed by file name.

    Returns
    -------
    list[str]
        One message per scope a local callee job requests above the grant of
        the job that calls it; empty when every call is within its grant.
    """
    return [
        shortfall
        for name, job_id, granted, callee in _local_calls(documents)
        for shortfall in _shortfalls(
            f"{name}:{job_id}", granted, documents[callee], callee
        )
    ]


def _local_calls(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> list[tuple[str, str, dict[str, int], str]]:
    """Return each local call with the caller's grant and the callee's name.

    Calls whose callee is not in the tree, and calls from a job whose grant is
    the unjudgeable default, are left out.
    """
    calls = [
        (name, job_id, _job_scopes(document, job), _local_name(job.get("uses")))
        for name, document in sorted(documents.items())
        for job_id, job in _jobs(document).items()
    ]
    return [call for call in calls if _is_judgeable(call, documents)]


def _is_judgeable(
    call: tuple[str, str, dict[str, int] | None, str | None],
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> bool:
    """Return whether a call has a readable grant and a callee in the tree."""
    _, _, granted, callee = call
    if granted is None or callee is None:
        return False
    return callee in documents


def _shortfalls(
    caller: str,
    granted: dict[str, int],
    callee_document: cabc.Mapping[str, object],
    callee: str,
) -> list[str]:
    """Return the scopes any job of one callee requests beyond `granted`."""
    return [
        f"{caller} grants {scope}: {_name(_granted(granted, scope))} but "
        f"{callee}:{job_id} requests {_name(level)}"
        for job_id, job in _jobs(callee_document).items()
        for scope, level in sorted((_job_scopes(callee_document, job) or {}).items())
        if level > _granted(granted, scope)
    ]


def _name(level: int) -> str:
    """Return the access level's name."""
    return next(name for name, value in LEVELS.items() if value == level)


def _pair(caller_permissions: str, callee_job_permissions: str) -> dict[str, dict]:
    """Return a caller and a callee with the given `permissions` blocks."""
    import yaml  # noqa: PLC0415 - only the constructed cases need it.

    caller = yaml.safe_load(
        "on: pull_request\n"
        + caller_permissions
        + "jobs:\n  call:\n    uses: ./.github/workflows/callee.yml\n"
    )
    callee = yaml.safe_load(
        "on: workflow_call\njobs:\n  work:\n    runs-on: x\n"
        + callee_job_permissions
        + "    steps:\n      - run: 'true'\n"
    )
    return {"caller.yml": caller, "callee.yml": callee}


@pytest.mark.parametrize(
    ("caller", "callee"),
    [
        pytest.param(
            "permissions:\n  contents: read\n",
            "    permissions:\n      contents: write\n",
            id="write-over-read",
        ),
        pytest.param(
            "permissions:\n  contents: read\n",
            "    permissions:\n      id-token: write\n",
            id="scope-not-granted",
        ),
        pytest.param(
            "permissions: read-all\n",
            "    permissions:\n      contents: write\n",
            id="write-over-read-all",
        ),
        pytest.param(
            "permissions: {}\n",
            "    permissions:\n      contents: read\n",
            id="read-over-none",
        ),
    ],
)
def test_a_callee_job_requesting_more_than_the_caller_grants_is_refused(
    caller: str, callee: str
) -> None:
    """The startup failure this contract exists for, in each shape it takes."""
    assert permission_shortfalls(_pair(caller, callee)), "the shortfall must be found"


@pytest.mark.parametrize(
    ("caller", "callee"),
    [
        pytest.param(
            "permissions:\n  contents: write\n",
            "    permissions:\n      contents: write\n",
            id="equal",
        ),
        pytest.param(
            "permissions:\n  contents: write\n",
            "    permissions:\n      contents: read\n",
            id="write-covers-read",
        ),
        pytest.param(
            "permissions: write-all\n",
            "    permissions:\n      id-token: write\n",
            id="write-all-covers",
        ),
        pytest.param("permissions:\n  contents: read\n", "", id="callee-asks-nothing"),
        pytest.param(
            "",
            "    permissions:\n      contents: write\n",
            id="caller-default-not-judged",
        ),
    ],
)
def test_a_callee_within_the_callers_grant_is_accepted(
    caller: str, callee: str
) -> None:
    """The refusal stays narrow: equal or lower requests, and an unjudgeable default."""
    assert permission_shortfalls(_pair(caller, callee)) == [], "no shortfall expected"


def test_a_job_level_grant_on_the_caller_overrides_the_workflow_cap() -> None:
    """The calling job's own block, not the workflow's, is what the callee is held to."""
    documents = _pair(
        "permissions:\n  contents: read\n", "    permissions:\n      contents: write\n"
    )
    documents["caller.yml"]["jobs"]["call"]["permissions"] = {"contents": "write"}
    assert permission_shortfalls(documents) == [], "the job-level grant must count"


def test_every_local_call_in_this_repository_is_within_its_grant() -> None:
    """Hold the real workflows to the rule, so the dry run starts on every pull request."""
    documents = repository_documents()
    assert permission_shortfalls(documents) == [], "a callee requests more than granted"


#: The condition that skips a job in a dry run.
DRY_RUN_SKIP: typ.Final = "should_publish"


def dry_run_write_defects(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]], entry: str
) -> list[str]:
    """Return the jobs a pull-request dry run could run with write access.

    The calling job's grant is the ceiling for the whole called workflow, so a
    callee job with no `permissions` of its own could hold it. Every job in
    the chain from `entry` must therefore declare its own block, and one that
    holds write must be skipped under the dry-run condition.
    """
    return [
        defect
        for name in _chain_from(documents, entry)
        for job_id, job in _jobs(documents[name]).items()
        for defect in _job_defects(f"{name}:{job_id}", job)
    ]


def _chain_from(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]], entry: str
) -> list[str]:
    """Return the workflows reachable from `entry` through local calls, in order."""
    order: list[str] = []
    pending = [entry]
    while pending:
        name = pending.pop()
        if name in order or name not in documents:
            continue
        order.append(name)
        pending.extend(_local_callees(documents[name]))
    return order


def _local_callees(document: cabc.Mapping[str, object]) -> list[str]:
    """Return the local workflows a workflow's jobs call."""
    names = (_local_name(job.get("uses")) for job in _jobs(document).values())
    return [name for name in names if name is not None]


def _holds_write(scopes: dict[str, int]) -> bool:
    """Return whether any scope is granted above read."""
    return any(level > LEVELS["read"] for level in scopes.values())


def _is_skipped_in_a_dry_run(job: cabc.Mapping[str, object]) -> bool:
    """Return whether the job's own condition names the dry-run skip."""
    return DRY_RUN_SKIP in str(job.get("if", ""))


def _needs_no_write_check(job: cabc.Mapping[str, object], own: dict[str, int]) -> bool:
    """Return whether a job may hold its grant: it calls on, reads, or is skipped."""
    if "uses" in job:
        return True
    return not _holds_write(own) or _is_skipped_in_a_dry_run(job)


def _job_defects(coordinate: str, job: cabc.Mapping[str, object]) -> list[str]:
    """Return what is wrong with one job's own permissions.

    A job that calls another workflow only sets the ceiling for it, so it is
    held to declaring one; the jobs it calls are held to the write rule.
    """
    own = _scopes(job.get("permissions"))
    if own is None:
        return [f"{coordinate} declares no permissions of its own"]
    if _needs_no_write_check(job, own):
        return []
    return [f"{coordinate} holds write but is not skipped in a dry run"]


def _chain(write_if: str, drop_block: bool) -> dict[str, dict]:
    """Return a dry-run caller and callee for the constructed cases."""
    import yaml  # noqa: PLC0415 - only the constructed cases need it.

    perms = "" if drop_block else "    permissions:\n      contents: read\n"
    caller = yaml.safe_load(
        "on: pull_request\njobs:\n  run:\n    permissions:\n      contents: write\n"
        "    uses: ./.github/workflows/callee.yml\n"
    )
    callee = yaml.safe_load(
        "on: workflow_call\njobs:\n  build:\n    runs-on: x\n"
        + perms
        + "    steps:\n      - run: 'true'\n  ship:\n    runs-on: x\n"
        + write_if
        + "    permissions:\n      contents: write\n"
        "    steps:\n      - run: 'true'\n"
    )
    return {"caller.yml": caller, "callee.yml": callee}


def test_a_callee_job_without_its_own_permissions_is_refused() -> None:
    """Deleting one job's read block leaves it holding the caller's ceiling."""
    documents = _chain("    if: needs.m.outputs.should_publish == 'true'\n", True)
    assert dry_run_write_defects(documents, "caller.yml") == [
        "callee.yml:build declares no permissions of its own"
    ]


def test_a_write_job_not_skipped_in_a_dry_run_is_refused() -> None:
    """Only a job the dry-run condition skips may hold write."""
    documents = _chain("", False)
    assert dry_run_write_defects(documents, "caller.yml") == [
        "callee.yml:ship holds write but is not skipped in a dry run"
    ]


def test_read_jobs_and_a_skipped_write_job_are_accepted() -> None:
    """The narrow case: read blocks everywhere, write only where skipped."""
    documents = _chain("    if: needs.m.outputs.should_publish == 'true'\n", False)
    assert dry_run_write_defects(documents, "caller.yml") == [], "no defect expected"


def test_only_a_skipped_job_holds_write_in_this_repository_dry_run() -> None:
    """Hold the real dry-run chain, including the workflows it calls in turn."""
    documents = repository_documents()
    assert dry_run_write_defects(documents, "release-dry-run.yml") == [], "write defect"


#: Every shape a `permissions` value can take for the two scopes the cases use.
_SCOPES: typ.Final = ("contents", "id-token")
_LEVEL_NAMES: typ.Final = ("none", "read", "write")


def _expected_shortfall(caller: dict[str, int], callee: dict[str, int]) -> bool:
    """Model the rule directly: some scope is requested above what is granted."""
    return any(callee.get(scope, 0) > caller.get(scope, 0) for scope in _SCOPES)


def _every_grant() -> list[dict[str, int]]:
    """Return every assignment of a level to each scope, as level numbers."""
    return [{"contents": a, "id-token": b} for a in range(3) for b in range(3)]


def _as_permissions(grant: dict[str, int]) -> dict[str, str]:
    """Write a numeric grant as a `permissions` mapping."""
    return {scope: _LEVEL_NAMES[level] for scope, level in grant.items()}


def test_the_shortfall_matches_a_direct_model_for_every_pair_of_grants() -> None:
    """Exhaust caller and callee grants over both scopes and all three levels.

    The invariant ranges over arbitrary mappings, but the space that matters is
    finite (two scopes, three levels, on either side), so it is checked in
    full against a model written independently of the implementation, rather
    than sampled. Job-level grants are exercised on both sides, and the
    workflow-level default for the callee.
    """
    for caller in _every_grant():
        for callee in _every_grant():
            documents = _pair("", "")
            documents["caller.yml"]["jobs"]["call"]["permissions"] = _as_permissions(
                caller
            )
            documents["callee.yml"]["jobs"]["work"]["permissions"] = _as_permissions(
                callee
            )
            found = bool(permission_shortfalls(documents))
            assert found == _expected_shortfall(caller, callee), (caller, callee)


def test_wildcard_grants_cover_or_fall_short_as_the_levels_say() -> None:
    """`read-all` and `write-all` on the caller cover exactly what their level covers."""
    for wildcard, level in (("read-all", 1), ("write-all", 2)):
        for callee in _every_grant():
            documents = _pair("", "")
            documents["caller.yml"]["jobs"]["call"]["permissions"] = wildcard
            documents["callee.yml"]["jobs"]["work"]["permissions"] = _as_permissions(
                callee
            )
            expected = any(want > level for want in callee.values())
            assert bool(permission_shortfalls(documents)) == expected, (
                wildcard,
                callee,
            )
