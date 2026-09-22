"""CodeScene coverage is owned by `main`, asserted against the workflows.

A pull request cannot upload coverage to CodeScene: uploads are accepted only
for analysed branches. What a pull-request lane can do is hand CodeScene a
credential and ask it to judge the branch, which is a different thing wearing
the same name, and it costs a secret on every fork-adjacent run and a check
whose verdict nobody on `main` ever sees.

So the boundary is one-directional and stated here rather than left to a
comment: no workflow a pull request can run may call a CodeScene action, run a
`cs-coverage` command, contact CodeScene, or reach `CS_ACCESS_TOKEN` at any
scope, and exactly one push-to-`main` workflow does the upload.

Both halves are asserted. Without the second, deleting the publisher outright
would satisfy the first and leave CodeScene with no coverage at all, which is
the failure the boundary exists to prevent rather than the one it is named for.

The readers live in `workflow_surface.py`. This module applies them to the
repository's own workflows; `test_workflow_surface.py` drives them with
constructed ones, because the repository declares only the shapes the contract
accepts.
"""

from __future__ import annotations

import typing as typ
from pathlib import Path

import pytest
from shell_commands import runs_command
from workflow_surface import (
    is_workflow_file,
    load,
    pull_request_surface,
    secret_breaches,
    steps,
    triggers,
)

WORKFLOWS: typ.Final = Path(__file__).resolve().parents[2] / ".github" / "workflows"

# The upload lives here and nowhere else.
PUBLISHER: typ.Final = "coverage-main.yml"

# What a pull-request lane may not call or run. Each is a distinct route to the
# same defect, so each is named rather than folded into one substring search.
CODESCENE_ACTION: typ.Final = "codescene"
CODESCENE_COMMAND: typ.Final = "cs-coverage"

# The action that performs the upload, searched for across every workflow so
# that a second publisher cannot appear unnoticed.
UPLOAD_ACTION: typ.Final = "upload-codescene-coverage"

# What the upload step must be handed, and the target that runs this contract.
SECRET: typ.Final = "${{ secrets.CS_ACCESS_TOKEN }}"
BOUNDARY_TARGET: typ.Final = "make test-codescene-boundary"


def _sources() -> dict[str, str]:
    """Every workflow in this repository, as source text."""
    found = {
        path.name: path.read_text(encoding="utf-8")
        for path in sorted(WORKFLOWS.iterdir())
        if path.is_file() and is_workflow_file(path.name)
    }
    assert found, "this contract is meaningless if no workflow was read"
    return found


def _documents() -> dict[str, dict[str, object]]:
    """Every workflow in this repository, parsed by the strict loader."""
    return {name: load(source) for name, source in _sources().items()}


def _values(workflow: str, key: str) -> list[str]:
    """Every `uses` or `run` body in a workflow, lowercased."""
    return [
        str(step[key]).lower() for step in steps(_documents()[workflow]) if key in step
    ]


PULL_REQUEST_WORKFLOWS: typ.Final = pull_request_surface(_documents())


def test_some_workflow_runs_on_pull_requests() -> None:
    """The guard on the guard.

    Every assertion below iterates the pull-request surface. An empty list
    would pass all of them while asserting nothing, which is how a reader
    defect turns a contract into decoration.
    """
    assert PULL_REQUEST_WORKFLOWS, "no workflow was classified as pull-request"


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
    that naming CodeScene in a step name does not read as a breach.
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
def test_no_pull_request_workflow_reaches_the_token_or_the_host(workflow: str) -> None:
    """No pull-request lane reaches the token or contacts CodeScene.

    That includes inheriting every secret into a workflow this tree cannot
    read, which names nothing and so is read from the document.
    """
    breaches = secret_breaches(_documents()[workflow], _sources()[workflow])
    assert not breaches, (
        f"{workflow} {'; '.join(breaches)}; coverage is owned by the "
        "push-to-main publisher"
    )


def test_the_coverage_job_takes_a_shallow_checkout() -> None:
    """The full clone went with the step that needed it.

    `fetch-depth: 0` was on this job's checkout so `cs-coverage check` could
    diff against the merge base. The coverage ratchet keeps its baseline in
    `actions/cache` and reads no history, so the absence is asserted on this
    job alone; another lane may have a real reason for a full clone.
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


def test_the_publisher_runs_on_a_push_to_main_and_nothing_else() -> None:
    """The other direction: something must still upload, and only from `main`.

    The upload step carries no ref guard of its own, so the trigger is the
    guard, and it is asserted by equality over the whole `on:` block. Asserting
    only that the push entry names `main` would pass a publisher that also
    answered `workflow_dispatch`, from which any branch could upload.
    """
    assert triggers(_documents()[PUBLISHER]) == {"push": {"branches": ["main"]}}, (
        f"{PUBLISHER} must run on a push to main and on nothing else"
    )


def test_the_publisher_queues_rather_than_cancels() -> None:
    """A cancelled publisher abandons its upload and its ratchet baseline.

    A queued one publishes later and the later push's baseline wins, so a
    concurrency group is welcome here and `cancel-in-progress: true` is not,
    at the workflow scope or on any job.
    """
    document = _documents()[PUBLISHER]
    scopes = [document, *(job for job in (document.get("jobs") or {}).values())]
    cancelling = [
        scope.get("concurrency")
        for scope in scopes
        if isinstance(scope.get("concurrency"), dict)
        and scope["concurrency"].get("cancel-in-progress") not in (None, False)
    ]
    assert not cancelling, f"{PUBLISHER} must not cancel a run in progress"


def test_exactly_one_workflow_uploads() -> None:
    """The publisher is the only one, not merely one that exists.

    The uploader is found by searching every workflow, and the set of
    workflows carrying one is asserted to be exactly the publisher, so a second
    push-to-main workflow with its own upload step cannot pass unnoticed.
    """
    uploaders = {
        name
        for name in _documents()
        if any(UPLOAD_ACTION in used for used in _values(name, "uses"))
    }
    assert uploaders == {PUBLISHER}, (
        f"exactly one workflow may upload coverage; found {sorted(uploaders)}"
    )


def _is_upload_step(step: dict[str, object]) -> bool:
    """Whether a step calls the upload action, read from its `uses` path."""
    path = str(step.get("uses", "")).partition("@")[0].strip()
    return path.rsplit("/", 1)[-1] == UPLOAD_ACTION


def _supplies_the_token(step: dict[str, object]) -> bool:
    """Whether a step hands the upload action the secret, directly or via env."""
    options = step.get("with")
    environment = step.get("env")
    given = (
        " ".join(str(options.get("access-token", "")).split())
        if isinstance(options, dict)
        else ""
    )
    via_env = (
        isinstance(environment, dict)
        and " ".join(str(environment.get("CS_ACCESS_TOKEN", "")).split()) == SECRET
    )
    return given == SECRET or (given == "${{ env.CS_ACCESS_TOKEN }}" and via_env)


def test_the_publisher_uploads_with_a_token() -> None:
    """And it must still do the upload it exists for, with the credential.

    Read from the steps rather than from the text: the action named in an
    `echo` or a comment, or the token named anywhere but the upload step's
    input, would satisfy a substring search while nothing uploaded.
    """
    uploads = [step for step in steps(_documents()[PUBLISHER]) if _is_upload_step(step)]
    assert uploads, f"{PUBLISHER} must call the upload action"
    assert all(_supplies_the_token(step) for step in uploads), (
        f"{PUBLISHER} must hand the upload action {SECRET}"
    )


def test_the_boundary_lane_runs_this_contract() -> None:
    """The contract is only a gate while a pull-request lane runs it.

    Read from a tokenized command line, so `echo make
    test-codescene-boundary`, or the target named in a comment, does not count
    as running it.
    """
    jobs = _documents()["ci.yml"].get("jobs")
    assert isinstance(jobs, dict), "ci.yml must declare jobs"
    job = jobs.get("docs-tooling")
    assert isinstance(job, dict), "ci.yml must declare the docs-tooling job"
    scripts = [
        str(step["run"])
        for step in job.get("steps", [])
        if isinstance(step, dict) and "run" in step
    ]
    assert any(runs_command(script, BOUNDARY_TARGET) for script in scripts), (
        f"docs-tooling must run `{BOUNDARY_TARGET}`"
    )
