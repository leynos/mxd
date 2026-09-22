"""CodeScene coverage is owned by `main`, asserted against the workflows.

A pull request cannot upload coverage to CodeScene: uploads are accepted only
for analysed branches. What a pull-request lane can do is hand CodeScene a
credential and ask it to judge the branch, which is a different thing wearing
the same name, and it costs a secret on every fork-adjacent run and a check
whose verdict nobody on `main` ever sees.

So the boundary is one-directional and stated here rather than left to a
comment: no pull-request workflow may call a CodeScene action, run a
`cs-coverage` command, or receive `CS_ACCESS_TOKEN` at any scope, and exactly
one push-to-`main` workflow does the upload.

Both halves are asserted. Without the second, deleting the publisher outright
would satisfy the first and leave CodeScene with no coverage at all, which is
the failure the boundary exists to prevent rather than the one it is named for.
"""

from __future__ import annotations

import re
import typing as typ
from pathlib import Path

import pytest
import yaml

WORKFLOWS: typ.Final = Path(__file__).resolve().parents[2] / ".github" / "workflows"

# The upload lives here and nowhere else.
PUBLISHER: typ.Final = "coverage-main.yml"

# What a pull-request lane may not carry. Each is a distinct route to the same
# defect, so each is named rather than folded into one substring search.
CODESCENE_ACTION: typ.Final = "codescene"
CODESCENE_COMMAND: typ.Final = "cs-coverage"
CODESCENE_SECRET: typ.Final = "CS_ACCESS_TOKEN"

# The action that performs the upload, searched for across every workflow so
# that a second publisher cannot appear unnoticed.
UPLOAD_ACTION: typ.Final = "upload-codescene-coverage"


def _documents() -> dict[str, dict[str, object]]:
    """Every workflow in this repository, parsed."""
    found = {
        path.name: yaml.safe_load(path.read_text(encoding="utf-8"))
        for path in sorted(WORKFLOWS.glob("*.yml"))
    }
    assert found, "this contract is meaningless if no workflow was read"
    return found


def _triggers(workflow: dict[str, object]) -> dict[str, object]:
    """A workflow's `on:` block, as a mapping of event name to configuration.

    YAML resolves the bare key `on` to the boolean `True`, so a reader keyed on
    the string finds nothing and every workflow reads as triggered by nothing,
    which would make every assertion below vacuously true.

    All three forms GitHub accepts are normalized here. `on: push` is a string
    and `on: [push, pull_request]` is a list, and both are as valid as the
    mapping form. Stringifying the list produced one key named
    `"['push', 'pull_request']"`, so a workflow written that way was not
    recognized as a pull-request lane and escaped every prohibition below,
    while the other workflows kept the non-empty guard passing. An unsupported
    shape is refused rather than coerced, for the same reason.
    """
    for key in (True, "on"):
        if key not in workflow:
            continue
        match workflow[key]:
            case dict() as mapping:
                return mapping
            case str() as event:
                return {event: None}
            case list() as events if all(isinstance(e, str) for e in events):
                return dict.fromkeys(typ.cast("list[str]", events))
            case other:
                message = f"unsupported `on:` shape {other!r}"
                raise AssertionError(message)
    message = "a workflow with no `on:` block cannot be classified"
    raise AssertionError(message)


# A whole-line comment. Stripped before the secret is searched for, so that
# explaining the boundary in a comment does not read as breaching it.
COMMENT_LINE: typ.Final = re.compile(r"(?m)^\s*#.*$")


def _text(path: str) -> str:
    """A workflow's source, lowercased, with whole-line comments removed."""
    source = (WORKFLOWS / path).read_text(encoding="utf-8")
    return COMMENT_LINE.sub("", source).lower()


def _steps(document: dict[str, object]) -> list[dict[str, object]]:
    """Every step of every job, plus each job that is itself a call.

    A job calling a reusable workflow has no steps and carries its `uses` on
    the job, so a walker that descended only into `steps` would miss the one
    shape that can run another repository's code.
    """
    jobs = document.get("jobs")
    if not isinstance(jobs, dict):
        return []
    found: list[dict[str, object]] = []
    for job in jobs.values():
        if not isinstance(job, dict):
            continue
        if "uses" in job:
            found.append(job)
        steps = job.get("steps")
        if isinstance(steps, list):
            found.extend(step for step in steps if isinstance(step, dict))
    return found


def _values(workflow: str, key: str) -> list[str]:
    """Every `uses` or `run` body in a workflow, lowercased."""
    return [
        str(step[key]).lower() for step in _steps(_documents()[workflow]) if key in step
    ]


# Where this repository's workflows live, as a `uses:` value names them.
WORKFLOW_PREFIX: typ.Final = ".github/workflows/"


def _local_call(used: str) -> str | None:
    """The workflow a `uses:` value names in this repository, or `None`.

    Matched by shape rather than by prefix. GitHub documents the local form as
    `./.github/workflows/<name>`, but a matcher enumerating literal prefixes
    has to be extended for every variant anyone proposes, and each omission is
    a workflow silently outside the surface. So a leading `./` is stripped and
    what remains is asked whether it is a path under this repository's
    workflow directory.

    A call to another repository is not local and is left out: it carries an
    `owner/repo/` prefix and an `@ref`, so it cannot reach this directory.
    """
    candidate = used.strip().removeprefix("./")
    if not candidate.startswith(WORKFLOW_PREFIX) or "@" in candidate:
        return None
    name = candidate.removeprefix(WORKFLOW_PREFIX)
    return name or None


def _called_locally(workflow: str) -> list[str]:
    """The workflows a workflow calls from this repository."""
    return [
        name
        for step in _steps(_documents()[workflow])
        if (name := _local_call(str(step.get("uses", ""))))
    ]


def _pull_request_surface() -> tuple[str, ...]:
    """Every workflow a pull request can run, calls included.

    A workflow triggered by `pull_request` is only the entry point. A job that
    calls a local reusable workflow runs that workflow's jobs with the caller's
    trigger, and `release-dry-run.yml` does exactly this, with
    `secrets: inherit`. A reader that stopped at the caller's own `uses` would
    pass while CodeScene ran on every pull request from inside `release.yml`.

    Followed transitively, since a called workflow may call another, and with
    a seen set so a cycle cannot hang the collection.
    """
    documents = _documents()
    pending = [
        name
        for name, document in documents.items()
        if "pull_request" in _triggers(document)
    ]
    reached: list[str] = []
    while pending:
        name = pending.pop()
        if name in reached or name not in documents:
            continue
        reached.append(name)
        pending.extend(_called_locally(name))
    return tuple(sorted(reached))


PULL_REQUEST_WORKFLOWS: typ.Final = _pull_request_surface()


def test_some_workflow_runs_on_pull_requests() -> None:
    """The guard on the guard.

    Every assertion below iterates the pull-request surface. An empty list
    would pass all of them while asserting nothing, which is how a reader
    defect turns a contract into decoration.
    """
    assert PULL_REQUEST_WORKFLOWS, "no workflow was classified as pull-request"


@pytest.mark.parametrize(
    ("declared", "expected"),
    [
        pytest.param({"pull_request": None}, ["pull_request"], id="mapping"),
        pytest.param("pull_request", ["pull_request"], id="string"),
        pytest.param(["push", "pull_request"], ["push", "pull_request"], id="list"),
    ],
)
def test_every_trigger_form_is_read_as_its_events(
    declared: object, expected: list[str]
) -> None:
    """All three forms GitHub accepts name the same events to this reader.

    The list form is the one that mattered. Stringifying it produced a single
    key named `"['push', 'pull_request']"`, so a workflow written that way was
    not recognized as a pull-request lane and escaped every prohibition, while
    the other workflows kept the non-empty guard passing. A hole that leaves
    the suite green is the only kind worth a case of its own.
    """
    assert list(_triggers({"on": declared})) == expected


def test_an_unsupported_trigger_shape_is_refused() -> None:
    """Refused rather than coerced, because coercion is what caused the hole."""
    with pytest.raises(AssertionError, match="unsupported"):
        _triggers({"on": 17})


@pytest.mark.parametrize(
    ("used", "expected"),
    [
        pytest.param("./.github/workflows/release.yml", "release.yml", id="documented"),
        pytest.param(".github/workflows/release.yml", "release.yml", id="no-dot"),
        pytest.param("  ./.github/workflows/release.yml  ", "release.yml", id="padded"),
        pytest.param("actions/checkout@v7.0.1", None, id="external-action"),
        pytest.param(
            "leynos/shared-actions/.github/workflows/x.yml@abc123",
            None,
            id="another-repository",
        ),
        pytest.param("./.github/actions/export-postgres-url", None, id="local-action"),
        pytest.param(".github/workflows/release.yml@abc123", None, id="path-with-ref"),
    ],
)
def test_a_local_call_is_recognized_by_shape(used: str, expected: str | None) -> None:
    """What counts as a call into this repository's own workflows.

    Matched by shape rather than by literal prefix: a matcher enumerating
    prefixes has to be extended for every variant anyone proposes, and each
    omission is a workflow silently outside the surface. The last two rows are
    the narrowness half, since a local *action* and a call to another
    repository must both stay out.
    """
    assert _local_call(used) == expected


def test_the_surface_follows_a_local_reusable_call() -> None:
    """The collection reaches past the caller, proved on the case that exists.

    `release-dry-run.yml` runs on `pull_request` and calls `release.yml` with
    `secrets: inherit`. If the surface held only the caller, every prohibition
    below could be breached inside the callee and this suite would stay green.
    """
    assert "release-dry-run.yml" in PULL_REQUEST_WORKFLOWS, (
        "the caller must be in the surface for this case to mean anything"
    )
    assert "release.yml" in PULL_REQUEST_WORKFLOWS, (
        "a workflow called from a pull-request lane runs on pull requests"
    )


@pytest.mark.parametrize("workflow", PULL_REQUEST_WORKFLOWS)
def test_no_pull_request_step_runs_a_codescene_action(workflow: str) -> None:
    """No pull-request lane calls a CodeScene action.

    Matched against each step's `uses` value rather than against the file, so
    that naming CodeScene in a step name or a comment, which is how the
    boundary gets explained where people read it, does not itself read as a
    breach.
    """
    offenders = [used for used in _values(workflow, "uses") if CODESCENE_ACTION in used]
    assert not offenders, (
        f"{workflow} calls {offenders!r}; coverage is owned by the "
        "push-to-main publisher"
    )


@pytest.mark.parametrize("workflow", PULL_REQUEST_WORKFLOWS)
def test_no_pull_request_step_runs_the_coverage_command(workflow: str) -> None:
    """No pull-request lane runs `cs-coverage`, however it was installed."""
    offenders = [body for body in _values(workflow, "run") if CODESCENE_COMMAND in body]
    assert not offenders, (
        f"{workflow} runs {CODESCENE_COMMAND!r}; coverage is owned by the "
        "push-to-main publisher"
    )


@pytest.mark.parametrize("workflow", PULL_REQUEST_WORKFLOWS)
def test_no_pull_request_workflow_receives_the_access_token(workflow: str) -> None:
    """The token is searched for in the source, and that is deliberate.

    A secret reaches a step through `env`, through `with`, through a job-level
    or workflow-level `env` block, or through an expression inside a `run`
    body. A reader that walked only one of those routes would pass on the
    others, and unlike an action or a command there is no legitimate reason for
    the name to appear on a pull-request lane at all.
    """
    assert CODESCENE_SECRET.lower() not in _text(workflow), (
        f"{workflow} receives {CODESCENE_SECRET}; coverage is owned by the "
        "push-to-main publisher"
    )


def test_the_coverage_job_takes_a_shallow_checkout() -> None:
    """The full clone went with the step that needed it.

    `fetch-depth: 0` was on this job's checkout so `cs-coverage check` could
    diff against the merge base. Nothing left in the job reads history: the
    coverage ratchet keeps its baseline in `actions/cache`. Asserting its
    absence is what stops the full clone creeping back with no step to justify
    it, and it is asserted on this job alone rather than repository-wide,
    because another lane may have a real reason for one.
    """
    jobs = _documents()["ci.yml"].get("jobs")
    assert isinstance(jobs, dict), "ci.yml must declare jobs"
    coverage = jobs.get("coverage")
    assert isinstance(coverage, dict), "ci.yml must declare a coverage job"
    checkouts = [
        step
        for step in coverage.get("steps", [])
        if isinstance(step, dict) and "checkout" in str(step.get("uses", ""))
    ]
    assert checkouts, "the coverage job must check the repository out"
    for step in checkouts:
        options = step.get("with") or {}
        assert "fetch-depth" not in options, (
            "the coverage job reads no git history; the full clone belonged to "
            "the CodeScene check step, which this repository no longer runs"
        )


def test_the_publisher_exists_and_runs_only_on_a_push_to_main() -> None:
    """The other direction: something must still upload.

    A boundary asserted in one direction is satisfied by deleting the thing it
    was protecting, so the publisher's existence and its trigger are asserted
    here. `pull_request` is refused explicitly, because a publisher that also
    ran on pull requests would satisfy every assertion above by sitting in a
    file whose name this contract trusts.
    """
    triggers = _triggers(_documents()[PUBLISHER])
    assert "push" in triggers, f"{PUBLISHER} must run on a push"
    assert triggers["push"] == {"branches": ["main"]}, (
        f"{PUBLISHER} must run on a push to main alone"
    )
    assert "pull_request" not in triggers, (
        f"{PUBLISHER} must not also run on a pull request"
    )


def test_exactly_one_workflow_uploads() -> None:
    """The publisher is the only one, not merely one that exists.

    Naming `coverage-main.yml` and asserting it uploads leaves a second
    push-to-main workflow with its own upload step invisible: coverage would be
    published twice, the work done twice, and this suite would pass. The
    uploader is therefore found by searching every workflow, and the set of
    workflows carrying one is asserted to be exactly the publisher.
    """
    uploaders = {
        name
        for name in _documents()
        if any(UPLOAD_ACTION in used for used in _values(name, "uses"))
    }
    assert uploaders == {PUBLISHER}, (
        f"exactly one workflow may upload coverage; found {sorted(uploaders)}"
    )


def test_the_publisher_uploads_with_a_token() -> None:
    """And it must still do the upload it exists for.

    Without this, the publisher could be reduced to a coverage run with the
    upload step deleted and every assertion here would still pass.
    """
    source = _text(PUBLISHER)
    assert "upload-codescene-coverage" in source, (
        f"{PUBLISHER} must call the upload action"
    )
    assert CODESCENE_SECRET.lower() in source, (
        f"{PUBLISHER} must receive the access token"
    )
