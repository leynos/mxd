"""The spelling gate, proved against a tree that must fail it.

`make spelling` runs on every pull request, and on a clean checkout it passes
whether or not it is enforcing anything. A flag dropped, a scope narrowed, or a
builder release that stops reading the phrase policy would all leave the lane
green and the policy unenforced, and nothing in the repository would say so.

So the gate is driven here against a fixture that holds a prohibited phrase.
The target itself is invoked, with only `SPELLING_ROOT` overridden, so the pin,
the subcommand and every flag are the ones a pull request runs. A test that
retyped the command would pass while the target drifted away from it.
"""

from __future__ import annotations

import shutil
import subprocess
import typing as typ
from pathlib import Path

import pytest

if typ.TYPE_CHECKING:
    import collections.abc as cabc

REPOSITORY: typ.Final = Path(__file__).resolve().parents[2]
# Declared in the shared dictionary's `[phrases.corrections]`. Split so this
# file does not itself carry the phrase the gate is asked to reject.
PROHIBITED: typ.Final = "hand" + "-written"
# The policy documents the gate reads, and the only two this repository tracks.
# The fixture carries its own, so the test measures this repository's policy
# rather than a stand-in.
#
# The shared dictionary is deliberately not among them. `.typos-oxendict-base
# .toml` and `.typos-oxendict-base.json` are the builder's download cache, are
# in `.gitignore`, and exist in a working tree only because some earlier
# command fetched them. Copying them made this test pass on a developer's
# machine and on a lane where `make spelling` happened to run first, and fail
# on a clean checkout or a reordered job. The builder fetches them into the
# fixture itself, which is what every repository relies on anyway.
POLICY_FILES: typ.Final = ("typos.toml", "typos.local.toml")


def _run_gate(root: Path) -> subprocess.CompletedProcess[str]:
    """Run the repository's own `spelling` target against a tree."""
    return subprocess.run(
        ["make", "spelling", f"SPELLING_ROOT={root}"],
        cwd=REPOSITORY,
        capture_output=True,
        text=True,
        check=False,
    )


@pytest.fixture
def tracked_tree(tmp_path: Path) -> cabc.Callable[[str], Path]:
    """Build a git tree carrying this repository's policy and one text file.

    The tree is tracked because the gate reads tracked files; an untracked
    fixture would be skipped and the test would pass for the wrong reason.

    Parameters
    ----------
    tmp_path : Path
        pytest's per-test directory. The fixture builds under it rather than
        beside the repository, so the builder's writes, which include
        refreshing the shared dictionary into the tree it is given, land
        somewhere pytest removes.

    Returns
    -------
    cabc.Callable[[str], Path]
        A builder taking the body of `docs/note.md` and returning the root of
        a git tree holding it, `typos.toml` and `typos.local.toml`, with
        everything staged. The body is the only thing a caller varies, because
        the policy is the repository's own and varying it would measure a
        stand-in.
    """

    def _build(body: str) -> Path:
        root = tmp_path / "fixture"
        root.mkdir()
        for name in POLICY_FILES:
            shutil.copy(REPOSITORY / name, root / name)
        (root / "docs").mkdir()
        (root / "docs" / "note.md").write_text(f"# Note\n\n{body}\n", encoding="utf-8")
        subprocess.run(["git", "init", "--quiet"], cwd=root, check=True)
        subprocess.run(["git", "add", "-A"], cwd=root, check=True)
        return root

    return _build


def test_a_prohibited_phrase_fails_the_gate(
    tracked_tree: cabc.Callable[[str], Path],
) -> None:
    """The unhappy path, which a clean checkout can never exercise.

    This is the assertion the deleted helper's test used to carry. Without it
    the lane proves only that the gate ran, not that it refuses anything.
    """
    result = _run_gate(tracked_tree(f"A {PROHIBITED} note."))
    assert result.returncode != 0, (
        "a tree holding a prohibited phrase must fail the spelling gate"
    )
    assert PROHIBITED in result.stdout + result.stderr, (
        "the gate must name the phrase it rejected, or nobody can act on it"
    )


def test_the_same_tree_without_the_phrase_passes(
    tracked_tree: cabc.Callable[[str], Path],
) -> None:
    """The narrowness half: the fixture is refused for the phrase, nothing else.

    Without this, a fixture that failed for an unrelated reason, a missing
    policy file or a malformed tree, would satisfy the test above while proving
    nothing about phrase enforcement.
    """
    result = _run_gate(tracked_tree("A handwritten note."))
    assert result.returncode == 0, (
        f"the fixture must pass once the phrase is corrected; "
        f"stdout={result.stdout!r} stderr={result.stderr!r}"
    )
