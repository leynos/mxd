#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.13"
# dependencies = ["cyclopts>=4,<5"]
# ///
"""Run the Loom models and require every expected model to pass.

A green ``cargo test`` is not evidence that any model ran. A run that forgot
``--cfg loom`` compiles every model out and passes with zero tests; a filter
that matches nothing passes the same way; and ``--no-run`` passes having
executed nothing at all. This script closes those holes by comparing what the
run reported against a committed list of model names, in both directions:

- every expected model must be reported as ``ok``;
- nothing may be reported as ``ok`` that the list does not name, so ordinary
  tests cannot stand in for models;
- and Cargo itself must exit zero.

An ignored model needs no rule of its own: it is not reported as ``ok``, so it
is already a missing model.

The Cargo command follows ``--`` so the Makefile, not this script, states it.
A summary goes to ``GITHUB_STEP_SUMMARY`` when that file is set.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import typing as typ

# Cyclopts resolves annotations at run time to coerce parameters, so `Path`
# must be a real import rather than a type-checking one.
from pathlib import Path  # noqa: TC003

from cyclopts import App, Parameter

if typ.TYPE_CHECKING:
    import collections.abc as cabc

app = App()

#: One libtest line reporting a passed test: ``test <name> ... ok``.
PASSED_LINE: typ.Final = re.compile(r"^test (?P<name>\S+) \.\.\. ok$")


def passed_tests(output: str) -> frozenset[str]:
    """Collect the names a libtest run reported as passed.

    >>> sorted(passed_tests("test a ... ok\\ntest b ... FAILED\\ntest c ... ignored"))
    ['a']

    Returns
    -------
    frozenset[str]
        The names of the tests reported ``ok``.
    """
    return frozenset(
        match["name"]
        for line in output.splitlines()
        if (match := PASSED_LINE.match(line.rstrip()))
    )


def read_expected(path: Path) -> frozenset[str]:
    """Read the expected model names, skipping blank lines and ``#`` comments.

    Returns
    -------
    frozenset[str]
        The names a run must report as passed.
    """
    lines = (line.strip() for line in path.read_text(encoding="utf-8").splitlines())
    return frozenset(line for line in lines if line and not line.startswith("#"))


def problems(
    returncode: int, passed: frozenset[str], expected: frozenset[str]
) -> list[str]:
    """Explain every way a run fails to show that each expected model passed.

    >>> problems(0, frozenset({"a"}), frozenset({"a"}))
    []

    Returns
    -------
    list[str]
        One sentence per problem, empty when the run is acceptable.
    """
    found: list[str] = []
    if not expected:
        found.append("the expected-model list is empty, so no run could prove anything")
    if returncode != 0:
        found.append(f"cargo test exited {returncode}")
    if missing := sorted(expected - passed):
        found.append(f"expected models not reported as passed: {', '.join(missing)}")
    if unexpected := sorted(passed - expected):
        found.append(
            f"passed tests the model list does not name: {', '.join(unexpected)}"
        )
    return found


def write_summary(
    summary: Path, passed: frozenset[str], expected: frozenset[str], found: list[str]
) -> None:
    """Append a Markdown summary of the run to the job summary file."""
    verdict = "failed" if found else "passed"
    lines = [
        f"## Loom models: {verdict}",
        "",
        f"- Expected: {len(expected)}; passed: {len(passed & expected)}",
        f"- Preemption bound: `{os.environ.get('LOOM_MAX_PREEMPTIONS', 'unset')}`",
        f"- `RUSTFLAGS`: `{os.environ.get('RUSTFLAGS', 'unset')}`",
        *(f"- {problem}" for problem in found),
        "",
    ]
    with summary.open("a", encoding="utf-8") as handle:
        handle.write("\n".join(lines))


@app.default
def main(
    *command: str,
    expected: typ.Annotated[Path, Parameter(name="--expected")],
) -> int:
    """Run ``command`` and check it reported every expected model as passed.

    Returns
    -------
    int
        Zero when every expected model passed and nothing else did.
    """
    if not command:
        print("run_loom_models: no cargo command given after --", file=sys.stderr)
        return 2
    run = subprocess.run(command, capture_output=True, text=True, check=False)  # noqa: S603
    sys.stdout.write(run.stdout)
    sys.stderr.write(run.stderr)
    passed = passed_tests(run.stdout)
    wanted = read_expected(expected)
    found = problems(run.returncode, passed, wanted)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        write_summary(Path(summary), passed, wanted, found)
    for problem in found:
        print(f"run_loom_models: {problem}", file=sys.stderr)
    return 1 if found else 0


def run_cli(argv: cabc.Sequence[str] | None = None) -> int:
    """Parse ``argv`` and run the checker, returning its exit status.

    Returns
    -------
    int
        The checker's exit status.
    """
    result = app(argv, result_action="return_value")
    return result if isinstance(result, int) else 1


if __name__ == "__main__":
    sys.exit(run_cli())
