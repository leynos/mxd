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

The readers live in `workflow_surface.py` and are driven here with constructed
workflows as well as this repository's own, because the repository declares
only the shapes the contract accepts.
"""

from __future__ import annotations

import typing as typ
from pathlib import Path

import pytest
from workflow_surface import (
    load,
    local_call,
    pull_request_surface,
    searchable,
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


def _sources() -> dict[str, str]:
    """Every workflow in this repository, as source text."""
    found = {
        path.name: path.read_text(encoding="utf-8")
        for path in sorted(WORKFLOWS.glob("*.yml"))
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

# The measured closure hole: a `workflow_call`-only workflow, called from a
# pull-request job with `secrets: inherit`, curling CodeScene with the token.
PROBE_CALLER: typ.Final = """\
on: pull_request
jobs:
  call:
    uses: {spelling}.github/workflows/probe.yml
    secrets: inherit
"""
PROBE: typ.Final = """\
on: workflow_call
jobs:
  probe:
    runs-on: ubuntu-latest
    steps:
      - run: >-
          curl -H "Authorization: Bearer ${{ secrets.CS_ACCESS_TOKEN }}"
          https://api.codescene.io/v2/projects
"""


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

    The list form is the one that mattered: stringified, it became one key and
    the workflow escaped every prohibition while the suite stayed green.
    """
    assert list(triggers({"on": declared})) == expected
    assert list(triggers(load(f"on: {declared!r}\njobs: {{}}\n"))) == expected


def test_an_unsupported_trigger_shape_is_refused() -> None:
    """Refused rather than coerced, because coercion is what caused the hole."""
    with pytest.raises(AssertionError, match="unsupported"):
        triggers({"on": 17})


def test_a_duplicate_key_is_refused_rather_than_resolved() -> None:
    """PyYAML keeps the last of two equal keys and says nothing.

    A lane declaring `runs-on` twice would be judged on the value GitHub may
    not use, so the loader refuses the file instead of picking one.
    """
    twice = "on: push\njobs:\n  a:\n    runs-on: x\n    runs-on: y\n"
    with pytest.raises(Exception, match="duplicate key 'runs-on'"):
        load(twice)
    assert load(twice.replace("    runs-on: y\n", ""))["jobs"] == {
        "a": {"runs-on": "x"}
    }


@pytest.mark.parametrize(
    ("used", "expected"),
    [
        pytest.param("./.github/workflows/release.yml", "release.yml", id="dot"),
        pytest.param("$/.github/workflows/release.yml", "release.yml", id="dollar"),
        pytest.param(".github/workflows/release.yml", "release.yml", id="bare"),
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

    GitHub documents both `./` and `$/` for a same-repository call. The last
    three rows are the narrowness half: a local *action*, a call to another
    repository, and a path carrying a ref must all stay out.
    """
    assert local_call(used) == expected


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


@pytest.mark.parametrize("spelling", ["./", "$/"])
def test_the_measured_probe_is_reached_and_refused(spelling: str) -> None:
    """The closure hole as it was measured elsewhere, in both call spellings.

    The probe declares only `workflow_call`, so a trigger reading never sees
    it; only following the caller's `uses` puts it on the surface, and only
    then does the token and host sweep read its `run` body.
    """
    sources = {"ci.yml": PROBE_CALLER.format(spelling=spelling), "probe.yml": PROBE}
    documents = {name: load(source) for name, source in sources.items()}
    assert pull_request_surface(documents) == ("ci.yml", "probe.yml")
    assert secret_breaches(documents["probe.yml"], sources["probe.yml"]) == [
        "names CS_ACCESS_TOKEN",
        "contacts codescene.io",
    ]


def test_the_surface_is_narrow() -> None:
    """A reusable workflow nothing calls is not on the surface.

    And `pull_request_target` is an entry point, since it runs for a pull
    request with the base repository's secrets.
    """
    documents = {
        "target.yml": load("on: pull_request_target\njobs: {}\n"),
        "unused.yml": load(PROBE),
        "main.yml": load("on: push\njobs: {}\n"),
    }
    assert pull_request_surface(documents) == ("target.yml",)


@pytest.mark.parametrize(
    ("source", "expected"),
    [
        pytest.param(
            "on: pull_request\njobs:\n  a:\n"
            "    uses: other/repo/.github/workflows/x.yml@abc\n"
            "    secrets: inherit\n",
            ["inherits every secret into 'other/repo/.github/workflows/x.yml@abc'"],
            id="inherit-to-another-repository",
        ),
        pytest.param(
            "on: pull_request\njobs:\n  a:\n"
            "    uses: ./.github/workflows/x.yml\n    secrets: inherit\n",
            [],
            id="inherit-to-a-local-call",
        ),
        pytest.param(
            "on: pull_request\njobs:\n  a:\n    steps:\n"
            "      - run: echo '${{ toJSON(secrets) }}'\n",
            ["reads the whole secrets context"],
            id="whole-context",
        ),
        pytest.param(
            "on: pull_request\njobs:\n  a:\n    uses: other/repo/.github/workflows/x.yml@abc\n"
            "    secrets:\n      token: ${{ secrets.CS_ACCESS_TOKEN }}\n",
            ["names CS_ACCESS_TOKEN"],
            id="named-forward",
        ),
        pytest.param(
            "on: pull_request\n# CS_ACCESS_TOKEN lives on main; see codescene.io\n"
            "jobs: {}\n",
            [],
            id="comment",
        ),
    ],
)
def test_the_token_sweep_reads_every_route(source: str, expected: list[str]) -> None:
    """Each route by which a pull-request lane could reach the token.

    `secrets: inherit` names nothing, so it is refused where the callee is in
    another repository and allowed where the callee is local, since a local
    callee is on the surface and read in turn. A comment is not a route.
    """
    assert secret_breaches(load(source), source) == expected


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


def test_the_publisher_uploads_with_a_token() -> None:
    """And it must still do the upload it exists for.

    Without this, the publisher could be reduced to a coverage run with the
    upload step deleted and every assertion here would still pass.
    """
    source = searchable(_sources()[PUBLISHER])
    assert UPLOAD_ACTION in source, f"{PUBLISHER} must call the upload action"
    assert "cs_access_token" in source, f"{PUBLISHER} must receive the access token"
