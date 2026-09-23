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
    load_workflow,
    repository_documents,
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
