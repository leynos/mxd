"""What the workflow reader refuses and records, driven with constructed files.

This repository's workflows declare none of these shapes, so a contract
parametrized over them would pass with each refusal deleted. Each case here
builds the shape and asserts the reader's answer directly.
"""

from __future__ import annotations

import typing as typ

import pytest
from ci_workflow_jobs import job_records
from ci_workflow_reader import (
    WorkflowLoadError,
    WorkflowShapeError,
    load_workflow,
    repository_documents,
    triggers,
    workflow_paths,
)
from test_gate_steps import _gating_steps
from test_runner_placement import is_external_call

if typ.TYPE_CHECKING:
    from pathlib import Path

JOB: typ.Final = "    runs-on: ubuntu-latest\n    timeout-minutes: 5\n"


def _records(tmp_path: Path, text: str) -> tuple[object, ...]:
    """Write one workflow and read its job records back."""
    path = tmp_path / "ci.yml"
    path.write_text(text, encoding="utf-8")
    return job_records({"ci.yml": load_workflow(path)})


def test_a_duplicate_key_fails_the_load(tmp_path: Path) -> None:
    """PyYAML would keep the second `runs-on`; the reader refuses the file."""
    path = tmp_path / "ci.yml"
    path.write_text(
        "on: push\njobs:\n  a:\n    runs-on: x\n    runs-on: y\n", encoding="utf-8"
    )
    with pytest.raises(WorkflowLoadError, match="duplicate key 'runs-on'"):
        load_workflow(path)


def test_both_suffixes_are_read_in_any_case(tmp_path: Path) -> None:
    """`CI.YML` and `release.yaml` are workflows; `notes.md` is not."""
    for name in ("CI.YML", "release.yaml", "notes.md"):
        (tmp_path / name).write_text("on: push\n", encoding="utf-8")
    assert [path.name for path in workflow_paths(tmp_path)] == [
        "CI.YML",
        "release.yaml",
    ]


@pytest.mark.parametrize(
    ("step", "expected"),
    [
        pytest.param("      - run: make check\n", (), id="plain"),
        pytest.param(
            "      - run: make check\n        shell: python {0}\n",
            ("shell",),
            id="shell",
        ),
        pytest.param(
            "      - run: make check\n        working-directory: vendor\n",
            ("working-directory",),
            id="directory",
        ),
    ],
)
def test_a_step_override_is_recorded_and_stops_it_gating(
    tmp_path: Path, step: str, expected: tuple[str, ...]
) -> None:
    """A step with its own shell or directory runs a different command."""
    (job,) = _records(tmp_path, f"on: push\njobs:\n  a:\n{JOB}    steps:\n{step}")
    assert job.steps[0].execution_overrides == expected
    assert _gating_steps(job, "make check") == (() if expected else (0,))


@pytest.mark.parametrize(
    ("workflow_defaults", "job_defaults", "expected"),
    [
        pytest.param("", "", (), id="none"),
        pytest.param(
            "defaults:\n  run:\n    shell: sh\n", "", ("workflow:shell",), id="workflow"
        ),
        pytest.param(
            "",
            "    defaults:\n      run:\n        working-directory: vendor\n",
            ("job:working-directory",),
            id="job",
        ),
    ],
)
def test_run_defaults_are_recorded_from_both_scopes(
    tmp_path: Path, workflow_defaults: str, job_defaults: str, expected: tuple[str, ...]
) -> None:
    """A `defaults.run` reaches every step below it, from either scope.

    So a job under one has no step that runs a gate command as written.
    """
    text = (
        f"on: push\n{workflow_defaults}jobs:\n  a:\n{JOB}{job_defaults}"
        "    steps:\n      - run: make check\n"
    )
    (job,) = _records(tmp_path, text)
    assert job.run_defaults == expected
    assert _gating_steps(job, "make check") == (() if expected else (0,))


@pytest.mark.parametrize(
    ("calls", "expected"),
    [
        pytest.param("./.github/workflows/release.yml", False, id="dot"),
        pytest.param("$/.github/workflows/release.yml", False, id="dollar"),
        pytest.param(
            "leynos/shared-actions/.github/workflows/x.yml@abc", True, id="other"
        ),
    ],
)
def test_both_local_spellings_are_this_repository(calls: str, expected: bool) -> None:
    """GitHub documents `./` and `$/` for a call that needs no pinned ref."""
    assert is_external_call(calls) is expected


def test_each_repository_read_is_a_fresh_snapshot() -> None:
    """One caller editing its documents cannot change what the next reads."""
    first = repository_documents()
    typ.cast("dict[str, object]", first).clear()
    assert repository_documents(), "a second read must not see the first's edit"


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        pytest.param("on:\n  pull_request:\n", ["pull_request"], id="mapping"),
        pytest.param("on: pull_request\n", ["pull_request"], id="event"),
        pytest.param("on: [push, pull_request]\n", ["push", "pull_request"], id="list"),
        pytest.param(
            "'on': [push, pull_request]\n", ["push", "pull_request"], id="quoted"
        ),
    ],
)
def test_every_trigger_form_names_its_events(
    tmp_path: Path, text: str, expected: list[str]
) -> None:
    """Each form and spelling GitHub accepts names the same events.

    A reader keyed on the boolean alone read a quoted `'on':` as no trigger,
    and a mapping-only reader stringified the list into one event name.
    """
    path = tmp_path / "ci.yml"
    path.write_text(f"{text}jobs: {{}}\n", encoding="utf-8")
    assert list(triggers(load_workflow(path))) == expected


@pytest.mark.parametrize(
    ("workflow", "match"),
    [
        pytest.param(
            {True: ["push"], "on": ["pull_request"]}, "exactly once", id="both"
        ),
        pytest.param({"jobs": {}}, "exactly once", id="absent"),
        pytest.param({True: 17}, "unsupported", id="number"),
        pytest.param(
            {True: ["push", {"pull_request": None}]}, "unsupported", id="mixed"
        ),
    ],
)
def test_a_trigger_block_it_cannot_read_whole_is_refused(
    workflow: dict[object, object], match: str
) -> None:
    """Refused rather than read in part, since GitHub merges both spellings."""
    with pytest.raises(WorkflowShapeError, match=match):
        triggers(typ.cast("dict[str, object]", workflow))
