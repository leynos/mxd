"""Every step that runs `cargo binstall` sends the workflow token, read-only.

binstall resolves a crate's release through `api.github.com`. An anonymous
request is rate limited by the runner's address, and a 403 makes binstall wait
two minutes and then build the crate from source, which took 348 s for
cargo-nextest on one CI leg while the other legs, warm, took a second. The
fallback is silent apart from a warning, so nothing fails: the job is merely
slow. Two contracts hold the line:

* No `cargo binstall` runs without the token. The judgement is
  :func:`unauthenticated_binstalls`, a query over a parsed workflow. It merges
  the workflow, job and step `env` in that order, so the most specific value
  wins, and `GITHUB_TOKEN` wins over `GH_TOKEN`. A token that is not the exact
  workflow token expression counts as no token, so a literal or an empty value
  does not pass.
* The token binstall sends can read the repository and nothing more.
  :func:`unrestricted_binstall_jobs` reports each job that runs `cargo
  binstall` without `permissions` of exactly `contents: read`, in its own block
  (which replaces the workflow's) or the workflow's.

The parsed workflows come from :func:`ci_workflow_reader.repository_documents`,
the one function that reads this repository. The property and execution
checks for the same query live in ``test_binstall_token_execution.py``.
"""

from __future__ import annotations

import re
import typing as typ

import pytest
from ci_workflow_reader import repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

    type Document = cabc.Mapping[str, object]

# `cargo +1.95.0 binstall` is valid rustup proxy syntax, so an optional
# `+toolchain` may sit between `cargo` and `binstall`.
BINSTALL: typ.Final = re.compile(r"\bcargo(?:\s+\+\S+)?\s+binstall\b")
TOKEN_EXPRESSION: typ.Final = "${{ github.token }}"


def _mapping(value: object) -> dict[str, object]:
    """Return `value` when it is a mapping, else an empty one."""
    return value if isinstance(value, dict) else {}


def _effective_token(*scopes: cabc.Mapping[str, object]) -> object:
    """Return the token value a step actually receives, or None.

    GitHub applies the most specific `env`, so the scopes are merged from the
    workflow down to the step and a later value replaces an earlier one, even
    when it is empty or a literal. `GITHUB_TOKEN` wins over `GH_TOKEN`.
    """
    merged: dict[str, object] = {}
    for scope in scopes:
        merged.update(_mapping(scope.get("env")))
    return merged.get("GITHUB_TOKEN", merged.get("GH_TOKEN"))


def unauthenticated_binstalls(document: Document) -> list[str]:
    """List the steps that run `cargo binstall` without the workflow token.

    Parameters
    ----------
    document : Document
        A parsed workflow.

    Returns
    -------
    list[str]
        One `job:step` coordinate per step that runs `cargo binstall`
        (optionally toolchain-qualified) whose effective `GITHUB_TOKEN` or
        `GH_TOKEN`, after workflow, job and step `env` are merged in that
        order, is not exactly the workflow token expression.
    """
    found: list[str] = []
    for job_id, job in _mapping(document.get("jobs")).items():
        steps = _mapping(job).get("steps")
        for step in steps if isinstance(steps, list) else []:
            step = _mapping(step)  # noqa: PLW2901 - narrowing a parsed value
            token = _effective_token(_mapping(document), _mapping(job), step)
            if BINSTALL.search(str(step.get("run", ""))) and token != TOKEN_EXPRESSION:
                found.append(f"{job_id}:{step.get('name')}")
    return found


def _binstall_steps(document: Document) -> list[str]:
    """List every step that runs `cargo binstall`, token or not."""
    return [
        f"{job_id}:{_mapping(step).get('name')}"
        for job_id, job in _mapping(document.get("jobs")).items()
        for step in _mapping(job).get("steps") or []
        if BINSTALL.search(str(_mapping(step).get("run", "")))
    ]


def unrestricted_binstall_jobs(document: Document) -> list[str]:
    """List the jobs running `cargo binstall` whose token is not read-only.

    Parameters
    ----------
    document : Document
        A parsed workflow.

    Returns
    -------
    list[str]
        The identifiers of jobs that run `cargo binstall` while neither the job
        nor the workflow declares `permissions` of exactly `contents: read`.
        A job's own block replaces the workflow's, as GitHub applies it.
    """
    read_only = {"contents": "read"}
    return [
        job_id
        for job_id, job in _mapping(document.get("jobs")).items()
        if any(
            BINSTALL.search(str(_mapping(step).get("run", "")))
            for step in _mapping(job).get("steps") or []
        )
        and _mapping(job).get("permissions", document.get("permissions")) != read_only
    ]


@pytest.fixture(scope="module")
def documents() -> cabc.Mapping[str, Document]:
    """This repository's parsed workflows."""
    return repository_documents()


def test_every_binstall_step_sends_the_token(
    documents: cabc.Mapping[str, Document],
) -> None:
    """No workflow runs `cargo binstall` anonymously."""
    offenders = {
        name: found
        for name, document in documents.items()
        if (found := unauthenticated_binstalls(document))
    }
    assert not offenders, f"cargo binstall without GITHUB_TOKEN: {offenders}"


def test_the_query_sees_the_binstall_steps(
    documents: cabc.Mapping[str, Document],
) -> None:
    """The query finds the steps it guards, so an empty result is not vacuous."""
    seen = {name: _binstall_steps(doc) for name, doc in documents.items()}
    assert {n: len(s) for n, s in seen.items() if s} == {
        "ci.yml": 3,
        "coverage-main.yml": 1,
        "audit.yml": 1,
    }, f"binstall steps found: {seen}"


def _document(step: dict[str, object], **extra: object) -> dict[str, object]:
    """Build a one-job workflow around `step`."""
    job: dict[str, object] = {"steps": [step], **extra}
    return {"jobs": {"j": job}}


@pytest.mark.parametrize(
    ("document", "expected"),
    [
        pytest.param(
            _document({"name": "a", "run": "cargo binstall x"}), ["j:a"], id="bare"
        ),
        pytest.param(
            _document(
                {
                    "name": "a",
                    "run": "cargo binstall x",
                    "env": {"GITHUB_TOKEN": "literal"},
                }
            ),
            ["j:a"],
            id="literal-token",
        ),
        pytest.param(
            _document(
                {"name": "a", "run": "cargo binstall x", "env": {"GITHUB_TOKEN": ""}}
            ),
            ["j:a"],
            id="empty-token",
        ),
        pytest.param(
            _document(
                {"name": "a", "run": "if :; then\n  cargo  binstall x\nfi"},
            ),
            ["j:a"],
            id="multiline-run",
        ),
        pytest.param(
            _document(
                {
                    "name": "a",
                    "run": "cargo binstall x",
                    "env": {"OTHER": TOKEN_EXPRESSION},
                }
            ),
            ["j:a"],
            id="wrong-variable",
        ),
        pytest.param(
            _document(
                {
                    "name": "a",
                    "run": "cargo binstall x",
                    "env": {"GITHUB_TOKEN": TOKEN_EXPRESSION},
                }
            ),
            [],
            id="step-token",
        ),
        pytest.param(
            _document(
                {"name": "a", "run": "cargo binstall x"},
                env={"GH_TOKEN": TOKEN_EXPRESSION},
            ),
            [],
            id="job-token",
        ),
        pytest.param(
            _document({"name": "a", "run": "cargo +1.95.0 binstall x"}),
            ["j:a"],
            id="toolchain-qualified",
        ),
        pytest.param(
            _document(
                {
                    "name": "a",
                    "run": "cargo +nightly binstall x",
                    "env": {"GITHUB_TOKEN": TOKEN_EXPRESSION},
                }
            ),
            [],
            id="toolchain-qualified-with-token",
        ),
        pytest.param(
            _document(
                {"name": "a", "run": "cargo binstall x", "env": {"GITHUB_TOKEN": ""}},
                env={"GITHUB_TOKEN": TOKEN_EXPRESSION},
            ),
            ["j:a"],
            id="step-empty-overrides-job-token",
        ),
        pytest.param(
            _document(
                {"name": "a", "run": "cargo binstall x", "env": {"GITHUB_TOKEN": "x"}},
                env={"GITHUB_TOKEN": TOKEN_EXPRESSION},
            ),
            ["j:a"],
            id="step-literal-overrides-job-token",
        ),
        pytest.param(
            _document(
                {
                    "name": "a",
                    "run": "cargo binstall x",
                    "env": {"GITHUB_TOKEN": TOKEN_EXPRESSION},
                },
                env={"GITHUB_TOKEN": "x"},
            ),
            [],
            id="step-token-overrides-job-literal",
        ),
        pytest.param(
            _document({"name": "a", "run": "cargo install x"}), [], id="not-binstall"
        ),
    ],
)
def test_the_query_judges_constructed_steps(
    document: Document, expected: list[str]
) -> None:
    """Each shape of step is judged as a token holder or not."""
    assert unauthenticated_binstalls(document) == expected, (
        f"wrong verdict for {document}"
    )


def test_every_binstall_job_holds_a_read_only_token(
    documents: cabc.Mapping[str, Document],
) -> None:
    """The token sent to binstall can read the repository and nothing more."""
    offenders = {
        name: found
        for name, document in documents.items()
        if (found := unrestricted_binstall_jobs(document))
    }
    assert not offenders, f"binstall jobs without read-only permissions: {offenders}"


@pytest.mark.parametrize(
    ("document", "expected"),
    [
        pytest.param(
            {"jobs": {"j": {"steps": [{"run": "cargo binstall x"}]}}},
            ["j"],
            id="unset",
        ),
        pytest.param(
            {
                "permissions": {"contents": "read"},
                "jobs": {"j": {"steps": [{"run": "cargo binstall x"}]}},
            },
            [],
            id="workflow-read-only",
        ),
        pytest.param(
            {
                "permissions": {"contents": "read"},
                "jobs": {
                    "j": {
                        "permissions": {"contents": "write"},
                        "steps": [{"run": "cargo binstall x"}],
                    }
                },
            },
            ["j"],
            id="job-widens-workflow",
        ),
        pytest.param(
            {
                "jobs": {
                    "j": {
                        "permissions": {"contents": "read"},
                        "steps": [{"run": "cargo +nightly binstall x"}],
                    }
                }
            },
            [],
            id="job-read-only",
        ),
    ],
)
def test_the_permission_query_judges_constructed_jobs(
    document: Document, expected: list[str]
) -> None:
    """Each placement of `permissions` is judged as GitHub applies it."""
    assert unrestricted_binstall_jobs(document) == expected, (
        f"wrong verdict for {document}"
    )
