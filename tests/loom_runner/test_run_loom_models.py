"""Behaviour of the Loom model runner.

The runner exists because ``cargo test`` passes when nothing ran: without
``--cfg loom`` every model is compiled out, a filter can match nothing, and
``--no-run`` executes nothing. These tests pin each of those as a failure,
and pin the one shape that must pass: every listed model reported ``ok`` and
nothing else.

``cmd-mox`` shims ``cargo`` so no Rust is compiled. Load the plugin with
``-p cmd_mox.pytest_plugin``, as ``make test-loom-runner`` does.
"""

from __future__ import annotations

import typing as typ

import pytest
import run_loom_models

if typ.TYPE_CHECKING:
    import pathlib

    from cmd_mox import CmdMox

manual_lifecycle = pytest.mark.cmd_mox(auto_lifecycle=False)

MODELS = ("loom_first_model", "loom_second_model")


def libtest_output(results: dict[str, str]) -> str:
    """Render libtest's plain-text output for the given name-to-outcome map.

    Returns
    -------
    str
        One ``test <name> ... <outcome>`` line per entry, then a result line.
    """
    lines = [f"test {name} ... {outcome}" for name, outcome in results.items()]
    passed = sum(outcome == "ok" for outcome in results.values())
    lines.append(f"test result: ok. {passed} passed; 0 failed")
    return "\n".join(lines) + "\n"


@pytest.fixture
def expected(tmp_path: pathlib.Path) -> pathlib.Path:
    """An expected-model list naming both models, with a comment line."""
    path = tmp_path / "loom-models.txt"
    path.write_text("# models\n" + "\n".join(MODELS) + "\n", encoding="utf-8")
    return path


def run_with_cargo(
    cmd_mox: CmdMox, expected: pathlib.Path, stdout: str, exit_code: int = 0
) -> int:
    """Run the checker against a shimmed ``cargo`` returning ``stdout``.

    Returns
    -------
    int
        The checker's exit status.
    """
    spy = cmd_mox.spy("cargo").returns(stdout=stdout, exit_code=exit_code)
    cmd_mox.replay()
    status = run_loom_models.run_cli(
        ["--expected", str(expected), "--", "cargo", "test", "--test", "loom_models"]
    )
    cmd_mox.verify()
    assert spy.call_count == 1, "the Cargo command must run exactly once"
    return status


@manual_lifecycle
def test_every_listed_model_passing_succeeds(
    cmd_mox: CmdMox, expected: pathlib.Path
) -> None:
    """Scenario: both listed models pass and nothing else runs.

    Invariant: this is the one report that is accepted.
    """
    output = libtest_output(dict.fromkeys(MODELS, "ok"))
    assert run_with_cargo(cmd_mox, expected, output) == 0


@manual_lifecycle
@pytest.mark.parametrize(
    ("case", "results", "exit_code"),
    [
        ("configuration dropped: nothing compiled in", {}, 0),
        ("one model missing", {MODELS[0]: "ok"}, 0),
        ("a model failed", {MODELS[0]: "ok", MODELS[1]: "FAILED"}, 101),
        (
            "an ordinary test stands in",
            {**dict.fromkeys(MODELS, "ok"), "ordinary_test": "ok"},
            0,
        ),
        ("a model ignored", {MODELS[0]: "ok", MODELS[1]: "ignored"}, 0),
        ("cargo failed after the models", dict.fromkeys(MODELS, "ok"), 101),
    ],
)
def test_a_report_that_does_not_prove_every_model_fails(
    cmd_mox: CmdMox,
    expected: pathlib.Path,
    case: str,
    results: dict[str, str],
    exit_code: int,
) -> None:
    """Scenario: a run that does not show every listed model passing.

    Invariant: the checker fails, whether or not Cargo itself exited zero.
    """
    status = run_with_cargo(cmd_mox, expected, libtest_output(results), exit_code)
    assert status == 1, f"{case}: the checker must fail"


def test_an_empty_model_list_is_refused(tmp_path: pathlib.Path) -> None:
    """Scenario: the list names no models.

    Invariant: an empty list is itself a problem, because every run satisfies
    it vacuously.
    """
    path = tmp_path / "loom-models.txt"
    path.write_text("# nothing listed\n", encoding="utf-8")
    expected = run_loom_models.read_expected(path)
    found = run_loom_models.problems(0, frozenset(), expected)
    assert found == ["the expected-model list is empty, so no run could prove anything"]


def test_no_command_is_a_usage_error(expected: pathlib.Path) -> None:
    """Scenario: nothing follows ``--``.

    Invariant: the checker refuses to run rather than checking an empty report.
    """
    assert run_loom_models.run_cli(["--expected", str(expected)]) == 2


def test_only_ok_lines_count_as_passed() -> None:
    """Scenario: a report mixes passed, failed and ignored tests.

    Invariant: only ``ok`` counts. The exit code alone would catch a failed
    model today, but a parser that also counted ``FAILED`` would leave the
    missing-model rule unable to see a model that did not pass.
    """
    output = libtest_output(
        {
            MODELS[0]: "ok",
            MODELS[1]: "FAILED",
            "ignored_test": "ignored",
        }
    )
    assert run_loom_models.passed_tests(output) == {MODELS[0]}
