"""Property and execution checks for the `cargo binstall` token contract.

:mod:`test_binstall_token` judges the repository's workflows and a table of
constructed steps. This module covers what a table cannot.

* A property test generates the workflow, job and step `env` mappings, with
  `GITHUB_TOKEN` and `GH_TOKEN` each absent, the exact token expression, empty
  or a literal, and holds the query to an oracle written separately from it.
* An execution test runs each real `cargo binstall` step's `run` script under
  `bash` with the environment GitHub would build for it, a `cargo` shim on the
  path, and a sentinel standing in for the workflow token. The shim records
  the token the process receives, which is the boundary where the 403 fix
  either reaches binstall or does not. What binstall then does with the token
  is cargo-binstall's behaviour and is out of reach offline.
"""

from __future__ import annotations

import os
import shutil
import stat
import subprocess  # noqa: S404 - the execution test runs a workflow step
import typing as typ

from ci_workflow_reader import repository_documents
from hypothesis import given, settings
from hypothesis import strategies as st
from test_binstall_token import (
    BINSTALL,
    TOKEN_EXPRESSION,
    unauthenticated_binstalls,
)

if typ.TYPE_CHECKING:
    import collections.abc as cabc
    from pathlib import Path

    type Document = cabc.Mapping[str, object]

SENTINEL: typ.Final = "sentinel-workflow-token"
TOKEN_VARIABLES: typ.Final = ("GITHUB_TOKEN", "GH_TOKEN")
# Printed by the shim for every `cargo binstall`; other cargo calls fail so a
# guard such as `cargo nextest --version` does not skip the install.
SHIM: typ.Final = """#!/bin/sh
case " $* " in
  *" binstall "*) printf 'token=%s\\n' "${GITHUB_TOKEN:-${GH_TOKEN:-}}" >> "$SHIM_LOG" ;;
  *) exit 1 ;;
esac
"""

ABSENT: typ.Final = object()
VALUES: typ.Final = st.sampled_from([ABSENT, TOKEN_EXPRESSION, "", "literal"])


def _env(values: cabc.Mapping[str, object]) -> dict[str, object]:
    """Keep the variables that are present."""
    return {k: v for k, v in values.items() if v is not ABSENT}


def _scope(values: cabc.Mapping[str, object]) -> dict[str, object]:
    """Build a workflow, job or step fragment from generated variables."""
    env = _env(values)
    return {"env": env} if env else {}


def _oracle(scopes: cabc.Sequence[cabc.Mapping[str, object]]) -> object:
    """Resolve the token from the most specific scope outwards.

    `scopes` runs from the step to the workflow. Each variable takes its first
    value found; `GITHUB_TOKEN` then outranks `GH_TOKEN`.
    """
    resolved = {
        name: next((s[name] for s in scopes if name in s), ABSENT)
        for name in TOKEN_VARIABLES
    }
    for name in TOKEN_VARIABLES:
        if resolved[name] is not ABSENT:
            return resolved[name]
    return ABSENT


_scope_values = st.fixed_dictionaries({name: VALUES for name in TOKEN_VARIABLES})


@settings(database=None, max_examples=300, deadline=None)
@given(workflow=_scope_values, job=_scope_values, step=_scope_values)
def test_effective_token_follows_scope_and_variable_precedence(
    workflow: dict[str, object], job: dict[str, object], step: dict[str, object]
) -> None:
    """A step is reported exactly when its effective token is not the workflow's."""
    document: dict[str, object] = {
        **_scope(workflow),
        "jobs": {
            "j": {
                **_scope(job),
                "steps": [{"name": "s", **_scope(step), "run": "cargo binstall x"}],
            }
        },
    }
    token = _oracle([_env(step), _env(job), _env(workflow)])
    expected = [] if token == TOKEN_EXPRESSION else ["j:s"]
    assert unauthenticated_binstalls(document) == expected, (
        f"wrong verdict for workflow={workflow} job={job} step={step}"
    )


def _merged_env(*scopes: cabc.Mapping[str, object]) -> dict[str, str]:
    """Build the environment GitHub gives a step, expressions resolved."""
    merged: dict[str, str] = {}
    for scope in scopes:
        env = scope.get("env")
        for name, value in (env if isinstance(env, dict) else {}).items():
            merged[str(name)] = str(value).replace(TOKEN_EXPRESSION, SENTINEL)
    return merged


def _jobs(document: Document) -> list[tuple[str, dict[str, typ.Any]]]:
    """List a workflow's `(job_id, job)` pairs."""
    jobs = document.get("jobs")
    return list(jobs.items()) if isinstance(jobs, dict) else []


def _binstall_steps(
    documents: cabc.Mapping[str, Document],
) -> list[tuple[str, str, dict[str, str], str]]:
    """Return `(coordinate, run, env, workflow)` for each binstall step."""
    candidates = [
        (name, document, job_id, job, step)
        for name, document in documents.items()
        for job_id, job in _jobs(document)
        for step in job.get("steps") or []
    ]
    return [
        (
            f"{name}:{job_id}:{step.get('name')}",
            str(step["run"]),
            _merged_env(document, job, step),
            name,
        )
        for name, document, job_id, job, step in candidates
        if BINSTALL.search(str(step.get("run", "")))
    ]


def _tokens_received(run: str, env: dict[str, str], tmp_path: Path) -> list[str]:
    """Run a step's script and return the token each `cargo binstall` saw."""
    shim_dir = tmp_path / "bin"
    shim_dir.mkdir(parents=True, exist_ok=True)
    shim = shim_dir / "cargo"
    shim.write_text(SHIM)
    shim.chmod(shim.stat().st_mode | stat.S_IXUSR)
    log = tmp_path / "shim.log"
    log.touch()
    # A bare environment: a token inherited from the developer's shell or the
    # runner must not stand in for the one the workflow configures.
    process_env = {
        "PATH": f"{shim_dir}:{os.defpath}",
        "SHIM_LOG": str(log),
        **env,
    }
    bash = shutil.which("bash")
    assert bash, "bash is required to run a workflow step"
    subprocess.run(  # noqa: S603 - fixed interpreter, repository-owned script
        [bash, "-e", "-c", run],
        env=process_env,
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    return [line.removeprefix("token=") for line in log.read_text().splitlines()]


def test_each_real_binstall_step_hands_binstall_the_workflow_token(
    tmp_path: Path,
) -> None:
    """The process a real step starts holds the workflow token when binstall runs."""
    steps = _binstall_steps(repository_documents())
    assert len(steps) == 5, f"expected five binstall steps, found {len(steps)}"
    for index, (coordinate, run, env, _) in enumerate(steps):
        received = _tokens_received(run, env, tmp_path / str(index))
        assert received == [SENTINEL], (
            f"{coordinate} gave cargo binstall {received}, not the workflow token"
        )


def test_the_execution_check_notices_a_step_without_the_token(
    tmp_path: Path,
) -> None:
    """Dropping the token from a real step is visible to the execution check."""
    coordinate, run, env, _ = _binstall_steps(repository_documents())[0]
    stripped = {k: v for k, v in env.items() if k not in TOKEN_VARIABLES}
    received = _tokens_received(run, stripped, tmp_path)
    assert received == [""], f"{coordinate} without a token gave {received}"
