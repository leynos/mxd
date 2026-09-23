"""The workflow readers, driven with constructed workflows.

This repository's own workflows declare only the shapes the boundary contract
accepts, so a reader proved only against them would pass with every refusal
deleted. Each case here builds the shape a reader must refuse, or must still
read, and asserts the reader's answer directly.
"""

from __future__ import annotations

import typing as typ

import pytest
from shell_commands import runs_command
from workflow_surface import (
    is_workflow_file,
    load,
    local_call,
    pull_request_surface,
    secret_breaches,
    triggers,
)

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


@pytest.mark.parametrize(
    ("name", "expected"),
    [
        pytest.param("ci.yml", True, id="yml"),
        pytest.param("release.yaml", True, id="yaml"),
        pytest.param("CI.YML", True, id="upper-case"),
        pytest.param("ci.yml.orig", False, id="backup"),
        pytest.param("README.md", False, id="markdown"),
    ],
)
def test_both_workflow_extensions_are_read(name: str, expected: bool) -> None:
    """A workflow the reader never opens is one every prohibition passes over."""
    assert is_workflow_file(name) is expected


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
        pytest.param(
            "$/.github/workflows/release.yml@abc123", None, id="dollar-with-ref"
        ),
    ],
)
def test_a_local_call_is_recognized_by_shape(used: str, expected: str | None) -> None:
    """What counts as a call into this repository's own workflows.

    GitHub documents both `./` and `$/` for a same-repository call. The last
    four rows are the narrowness half: a local *action*, a call to another
    repository, and a path carrying a ref in either spelling must all stay out.
    """
    assert local_call(used) == expected


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
    assert secret_breaches(documents["probe.yml"]) == [
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
        pytest.param(
            "on: pull_request\njobs:\n  a:\n    steps:\n"
            "      - uses: peter-evans/create-or-update-comment@v4\n"
            "        with:\n          body: |\n"
            "            # ${{ secrets.CS_ACCESS_TOKEN }}\n",
            ["names CS_ACCESS_TOKEN"],
            id="hash-line-in-a-block-scalar",
        ),
    ],
)
def test_the_token_sweep_reads_every_route(source: str, expected: list[str]) -> None:
    """Each route by which a pull-request lane could reach the token.

    `secrets: inherit` names nothing, so it is refused where the callee is in
    another repository and allowed where the callee is local, since a local
    callee is on the surface and read in turn. A comment is not a route, but a
    `#` line inside a block scalar is data, and Actions expands it.
    """
    assert secret_breaches(load(source)) == expected


@pytest.mark.parametrize(
    ("script", "expected"),
    [
        pytest.param("make test-codescene-boundary", True, id="bare"),
        pytest.param("make  test-codescene-boundary", True, id="two-spaces"),
        pytest.param("set -e && FOO=1 make test-codescene-boundary", True, id="list"),
        pytest.param("true \\\n  ; make test-codescene-boundary", True, id="continued"),
        pytest.param("echo make test-codescene-boundary", False, id="echo"),
        pytest.param("# make test-codescene-boundary", False, id="comment"),
        pytest.param("make test-codescene", False, id="other-target"),
        pytest.param("make 'test-codescene-boundary", False, id="unbalanced"),
    ],
)
def test_a_required_command_is_read_from_the_command_line(
    script: str, expected: bool
) -> None:
    """A lane runs a command only when a simple command begins with its words.

    A substring requirement passes for `echo make test-codescene-boundary`,
    which runs nothing.
    """
    assert runs_command(script, "make test-codescene-boundary") is expected


def _surface(sources: dict[str, str]) -> tuple[str, ...]:
    """The pull-request surface of a set of constructed workflows."""
    return pull_request_surface({name: load(text) for name, text in sources.items()})


CALLS: typ.Final = "jobs:\n  a:\n    uses: ./.github/workflows/{callee}\n"


def test_a_breach_two_calls_deep_is_reached_and_refused() -> None:
    """The closure is transitive: only the grandchild carries the token."""
    sources = {
        "ci.yml": "on: pull_request\n" + CALLS.format(callee="child.yml"),
        "child.yml": "on: workflow_call\n" + CALLS.format(callee="grandchild.yml"),
        "grandchild.yml": PROBE,
    }
    assert _surface(sources) == ("child.yml", "ci.yml", "grandchild.yml")
    assert secret_breaches(load(PROBE)) == [
        "names CS_ACCESS_TOKEN",
        "contacts codescene.io",
    ]


def test_a_call_cycle_terminates() -> None:
    """Two workflows calling each other are each collected once."""
    sources = {
        "ci.yml": "on: pull_request\n" + CALLS.format(callee="loop.yml"),
        "loop.yml": "on: workflow_call\n" + CALLS.format(callee="ci.yml"),
    }
    assert _surface(sources) == ("ci.yml", "loop.yml")


def test_a_workflow_run_chain_joins_only_when_it_follows_the_surface() -> None:
    """`workflow_run` after a pull-request lane runs with the repository's secrets.

    Its local calls join with it. A chain onto a push-only lane stays out, which
    is the narrowness half.
    """
    sources = {
        "ci.yml": "name: CI\non: pull_request\njobs: {}\n",
        "after.yml": (
            "on:\n  workflow_run:\n    workflows: [CI]\n"
            + CALLS.format(callee="report.yml")
        ),
        "report.yml": "on: workflow_call\njobs: {}\n",
        "nightly.yml": "name: Nightly\non: push\njobs: {}\n",
        "after-nightly.yml": (
            "on:\n  workflow_run:\n    workflows: [Nightly]\njobs: {}\n"
        ),
    }
    assert _surface(sources) == ("after.yml", "ci.yml", "report.yml")
