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

# The publisher's concurrency block, compared whole. Keyed on the ref alone:
# a group keyed on the event lets an earlier dispatch finish after a newer push
# and upload older coverage last, and a run-unique key never groups at all.
PUBLISHER_CONCURRENCY: typ.Final = {
    "group": "${{ github.workflow }}-${{ github.ref }}",
    "cancel-in-progress": False,
}

# The availability check. Its expression is evaluated before the shell runs,
# so the token enters no process and no `env`, and the step's only command
# writes `true` or `false`.
TOKEN_CHECK_ID: typ.Final = "codescene-token"
TOKEN_CHECK_COMMAND: typ.Final = (
    'echo "available=${{ secrets.CS_ACCESS_TOKEN != \'\' }}" >> "$GITHUB_OUTPUT"'
)
# The upload's whole condition. Compared by equality, so an appended `||`
# cannot widen it, and a guard on `env.CS_ACCESS_TOKEN`, which passes with
# the binding deleted, cannot replace it.
UPLOAD_CONDITION: typ.Final = (
    f"steps.{TOKEN_CHECK_ID}.outputs.available == 'true' "
    "&& github.ref == 'refs/heads/main'"
)


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
    breaches = secret_breaches(_documents()[workflow])
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


def test_the_publisher_never_overlaps_and_never_cancels() -> None:
    """One group per ref, never cancelled, at the workflow scope.

    A cancelled publisher abandons its upload and its ratchet baseline. With
    one group, runs never overlap and a newer push replaces a pending run.
    No job may declare a block of its own that would sit beside this one.
    """
    document = _documents()[PUBLISHER]
    assert document.get("concurrency") == PUBLISHER_CONCURRENCY, (
        f"{PUBLISHER} must declare concurrency {PUBLISHER_CONCURRENCY}, "
        f"found {document.get('concurrency')!r}"
    )
    job_blocks = [
        job.get("concurrency")
        for job in (document.get("jobs") or {}).values()
        if isinstance(job, dict) and "concurrency" in job
    ]
    assert not job_blocks, f"{PUBLISHER} jobs must not declare concurrency"


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


def _publisher_steps() -> list[dict[str, object]]:
    """The publisher's steps, in order."""
    return steps(_documents()[PUBLISHER])


def test_the_publisher_uploads_with_the_token_given_directly() -> None:
    """The upload is called once, handed the secret itself, on its condition.

    Read from the steps rather than from the text: the action named in an
    `echo` or a comment would satisfy a substring search while nothing
    uploaded.
    """
    uploads = [step for step in _publisher_steps() if _is_upload_step(step)]
    assert len(uploads) == 1, f"{PUBLISHER} must call the upload action once"
    (upload,) = uploads
    options = upload.get("with")
    assert isinstance(options, dict) and options.get("access-token") == SECRET, (
        f"the upload must be handed access-token: {SECRET} directly"
    )
    assert upload.get("if") == UPLOAD_CONDITION, (
        f"the upload's condition must be exactly {UPLOAD_CONDITION!r}, "
        f"found {upload.get('if')!r}"
    )


def test_the_token_check_runs_its_one_command_unconditionally() -> None:
    """Deleting or conditioning the check would skip the upload forever.

    The check precedes the upload, carries the id the condition reads, has no
    `if` and no `env`, and its whole `run` body is the one command.
    """
    publisher = _publisher_steps()
    checks = [i for i, step in enumerate(publisher) if step.get("id") == TOKEN_CHECK_ID]
    uploads = [i for i, step in enumerate(publisher) if _is_upload_step(step)]
    assert len(checks) == 1, f"{PUBLISHER} must declare one {TOKEN_CHECK_ID!r} step"
    assert uploads and checks[0] < uploads[0], "the check must precede the upload"
    check = publisher[checks[0]]
    assert "if" not in check, "the token check must be unconditional"
    assert "env" not in check, "the token check must bind nothing in env"
    assert str(check.get("run", "")).strip() == TOKEN_CHECK_COMMAND, (
        f"the token check's whole run body must be {TOKEN_CHECK_COMMAND!r}"
    )


def test_no_env_on_the_publisher_holds_the_token() -> None:
    """The upload action is composite and passes a step's env to its steps.

    So the token is refused in every `env` on the publisher: the workflow,
    each job, and each step. It reaches the action through `access-token`
    alone.
    """
    document = _documents()[PUBLISHER]
    job_list = [
        job for job in (document.get("jobs") or {}).values() if isinstance(job, dict)
    ]
    scopes = [document, *job_list, *_publisher_steps()]
    holding = [
        env
        for scope in scopes
        if isinstance(env := scope.get("env"), dict)
        and any("CS_ACCESS_TOKEN" in f"{key} {value}" for key, value in env.items())
    ]
    assert not holding, f"{PUBLISHER} binds the token in env: {holding}"


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
