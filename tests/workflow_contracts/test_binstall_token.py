"""Every step that runs `cargo binstall` sends the workflow token.

binstall resolves a crate's release through `api.github.com`. An anonymous
request is rate limited by the runner's address, and a 403 makes binstall wait
two minutes and then build the crate from source, which took 348 s for
cargo-nextest on one CI leg while the other legs, warm, took a second. The
fallback is silent apart from a warning, so nothing fails: the job is merely
slow. These contracts hold that no `cargo binstall` runs without a token.

The judgement is :func:`unauthenticated_binstalls`, a query over a parsed
workflow. It reads the step's own `env` and its job's `env`, and treats a token
that is not the workflow token expression as no token, so a literal or an empty
value does not count.
"""

from __future__ import annotations

import re
import typing as typ

import pytest
from ci_workflow_reader import repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

    type Document = cabc.Mapping[str, object]

BINSTALL: typ.Final = re.compile(r"\bcargo\s+binstall\b")
TOKEN_NAMES: typ.Final = ("GITHUB_TOKEN", "GH_TOKEN")
TOKEN_EXPRESSION: typ.Final = "${{ github.token }}"


def _mapping(value: object) -> dict[str, object]:
    """Return `value` when it is a mapping, else an empty one."""
    return value if isinstance(value, dict) else {}


def _has_token(*scopes: cabc.Mapping[str, object]) -> bool:
    """Say whether any scope's `env` sets a token name to the workflow token."""
    return any(
        _mapping(scope.get("env")).get(name) == TOKEN_EXPRESSION
        for scope in scopes
        for name in TOKEN_NAMES
    )


def unauthenticated_binstalls(document: Document) -> list[str]:
    """List the steps that run `cargo binstall` without the workflow token."""
    found: list[str] = []
    for job_id, job in _mapping(document.get("jobs")).items():
        steps = _mapping(job).get("steps")
        for step in steps if isinstance(steps, list) else []:
            step = _mapping(step)  # noqa: PLW2901 - narrowing a parsed value
            if BINSTALL.search(str(step.get("run", ""))) and not _has_token(
                _mapping(document), _mapping(job), step
            ):
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
