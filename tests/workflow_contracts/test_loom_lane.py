"""The Loom lane runs every model, and only a run of every model passes.

``make test-loom`` is the lane's gate, and ``test_gate_steps.py`` pins the
step that runs it. This file pins what the target does, read through
``make --dry-run`` so the assertion is on the command Make would execute
rather than on the Makefile's text:

- the model command is ``cargo test`` with ``--cfg loom`` in ``RUSTFLAGS``, a
  positive ``LOOM_MAX_PREEMPTIONS``, the ``mxd-concurrency`` package and each
  model target, and without ``--no-run``;
- it runs under ``scripts/run_loom_models.py``, which fails a run that does
  not report every listed model as passed;
- the model list names exactly the ``#[test]`` functions in the model targets,
  so a new model cannot be left out of the check and a deleted one cannot
  linger in it;
- the schedule is daily at 17:30 UTC with manual dispatch, and the
  exploration never runs on a pull request, where ``loom-check.yml`` compiles
  the models instead.
"""

from __future__ import annotations

import re
import shlex
import subprocess
import typing as typ

import pytest
from ci_workflow_reader import REPO_ROOT, repository_documents, triggers

if typ.TYPE_CHECKING:
    import collections.abc as cabc

MODEL_LIST: typ.Final = REPO_ROOT / "crates" / "mxd-concurrency" / "loom-models.txt"
MODEL_TESTS: typ.Final = REPO_ROOT / "crates" / "mxd-concurrency" / "tests"
RUNNER: typ.Final = "scripts/run_loom_models.py"
# A `#[test]` attribute followed by the function it marks.
TEST_FUNCTION: typ.Final = re.compile(r"#\[test\]\s*fn\s+(?P<name>\w+)")


def dry_run(target: str) -> list[list[str]]:
    """Return each command ``make --dry-run <target>`` prints, tokenized.

    Returns
    -------
    list[list[str]]
        One token list per recipe line.
    """
    printed = subprocess.run(
        ["make", "--no-print-directory", "--dry-run", target],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [shlex.split(line) for line in printed.splitlines() if line.strip()]


def split_environment(tokens: list[str]) -> tuple[dict[str, str], list[str]]:
    """Separate leading ``NAME=value`` assignments from the command.

    Returns
    -------
    tuple[dict[str, str], list[str]]
        The assignments, and the command that follows them.
    """
    environment: dict[str, str] = {}
    for index, token in enumerate(tokens):
        name, separator, value = token.partition("=")
        if not separator or not name.isidentifier():
            return environment, tokens[index:]
        environment[name] = value
    return environment, []


@pytest.fixture(scope="module")
def model_command() -> tuple[dict[str, str], list[str], list[str]]:
    """The test-loom recipe: its environment, runner argv and Cargo argv."""
    (line,) = dry_run("test-loom")
    environment, command = split_environment(line)
    assert "--" in command, (
        f"test-loom must run cargo under {RUNNER}, after its `--`: {command}"
    )
    separator = command.index("--")
    return environment, command[:separator], command[separator + 1 :]


def model_targets(cargo: cabc.Sequence[str]) -> list[str]:
    """Return every ``--test`` target a Cargo argument vector selects.

    Returns
    -------
    list[str]
        The target names, in order.
    """
    return [cargo[i + 1] for i, token in enumerate(cargo[:-1]) if token == "--test"]


def test_the_models_run_under_the_loom_configuration(
    model_command: tuple[dict[str, str], list[str], list[str]],
) -> None:
    """The model command is a bounded ``cargo test`` of the kernel crate."""
    environment, _, cargo = model_command
    assert cargo[:2] == ["cargo", "test"], f"not a cargo test run: {cargo}"
    assert "--cfg loom" in environment.get("RUSTFLAGS", ""), environment
    assert int(environment.get("LOOM_MAX_PREEMPTIONS", "0")) > 0, environment
    assert cargo[cargo.index("-p") + 1] == "mxd-concurrency", cargo
    assert "--no-run" not in cargo, "the lane must execute the models"
    assert model_targets(cargo), "the lane must name its model targets"


def test_the_runner_checks_the_model_list(
    model_command: tuple[dict[str, str], list[str], list[str]],
) -> None:
    """The Cargo command runs under the checker, given the committed list."""
    _, runner, _ = model_command
    assert RUNNER in runner, f"the models must run under {RUNNER}: {runner}"
    listed = runner[runner.index("--expected") + 1]
    assert REPO_ROOT / listed == MODEL_LIST, listed


def test_the_model_list_names_exactly_the_model_tests(
    model_command: tuple[dict[str, str], list[str], list[str]],
) -> None:
    """Every model in the selected targets is listed, and nothing else is."""
    _, _, cargo = model_command
    defined = set()
    for target in model_targets(cargo):
        source = (MODEL_TESTS / f"{target}.rs").read_text(encoding="utf-8")
        defined |= {match["name"] for match in TEST_FUNCTION.finditer(source)}
    listed = {
        line.strip()
        for line in MODEL_LIST.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.strip().startswith("#")
    }
    assert defined, "the model targets define no models"
    assert listed == defined, (
        f"listed but not defined: {sorted(listed - defined)}; "
        f"defined but not listed: {sorted(defined - listed)}"
    )


def test_the_smoke_check_compiles_without_exploring() -> None:
    """``check-loom`` builds the models under ``--cfg loom`` and runs none."""
    commands = [split_environment(line) for line in dry_run("check-loom")]
    compile_only = [
        (environment, command)
        for environment, command in commands
        if command[:2] == ["cargo", "test"]
    ]
    assert len(compile_only) == 1, commands
    ((environment, command),) = compile_only
    assert "--cfg loom" in environment.get("RUSTFLAGS", ""), environment
    assert "--no-run" in command, command


def test_exploration_runs_daily_and_on_demand_only() -> None:
    """The model lane is scheduled and dispatchable, and never a PR lane."""
    documents = repository_documents()
    events = triggers(documents["loom.yml"])
    assert set(events) == {"schedule", "workflow_dispatch"}, set(events)
    assert events["schedule"] == [{"cron": "30 17 * * *"}], events["schedule"]
    assert "pull_request" in triggers(documents["loom-check.yml"])
