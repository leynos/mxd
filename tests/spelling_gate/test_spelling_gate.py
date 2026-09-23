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
# US spellings two anchored exceptions quote. Split for the same reason.
API_COLOUR: typ.Final = "col" + "or"
API_FLAVOUR: typ.Final = "flav" + "or"
API_NORMALIZED: typ.Final = "normal" + "ised"
API_SERVER: typ.Final = "S" + "er"
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


def _write(root: Path, files: cabc.Mapping[str, str]) -> None:
    """Write each relative path's text under a root, creating directories."""
    for relative, text in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")


@pytest.fixture
def tracked_tree(tmp_path: Path) -> cabc.Callable[..., Path]:
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
    cabc.Callable[..., Path]
        A builder taking the body of `docs/note.md`, optionally further files
        to track and files to leave untracked, and returning the root of a git
        tree holding them, `typos.toml` and `typos.local.toml`, with every
        tracked file staged. The body is the only thing a caller varies, because
        the policy is the repository's own and varying it would measure a
        stand-in.
    """

    def _build(
        body: str,
        tracked: cabc.Mapping[str, str] | None = None,
        untracked: cabc.Mapping[str, str] | None = None,
    ) -> Path:
        root = tmp_path / "fixture"
        root.mkdir()
        for name in POLICY_FILES:
            shutil.copy(REPOSITORY / name, root / name)
        _write(root, {"docs/note.md": f"# Note\n\n{body}\n", **(tracked or {})})
        subprocess.run(["git", "init", "--quiet"], cwd=root, check=True)
        subprocess.run(["git", "add", "-A"], cwd=root, check=True)
        _write(root, untracked or {})
        return root

    return _build


def test_a_prohibited_phrase_fails_the_gate(
    tracked_tree: cabc.Callable[..., Path],
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
    tracked_tree: cabc.Callable[..., Path],
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


def test_a_prohibited_phrase_in_tracked_code_fails_the_gate(
    tracked_tree: cabc.Callable[..., Path],
) -> None:
    """The gate reads every tracked file, not Markdown alone."""
    root = tracked_tree(
        "A clean note.", tracked={"src/lib.rs": f"// A {PROHIBITED} comment.\n"}
    )
    result = _run_gate(root)
    assert result.returncode != 0, "a tracked source file must be checked too"
    assert "src/lib.rs" in result.stdout + result.stderr, (
        "the gate must name the source file it rejected"
    )


def test_an_untracked_file_is_not_read(
    tracked_tree: cabc.Callable[..., Path],
) -> None:
    """The gate's scope is the tracked tree, so scratch files cannot fail it."""
    root = tracked_tree(
        "A clean note.", untracked={"scratch.md": f"A {PROHIBITED} scratch note.\n"}
    )
    result = _run_gate(root)
    assert result.returncode == 0, (
        f"an untracked file must not be scanned; stdout={result.stdout!r}"
    )


# (body, the flagged word or None when the gate must pass). Each exception
# appears once inside its pattern and once in prose, where it is still checked.
INLINE_CODE_CASES: typ.Final = (
    pytest.param(f"The `{API_COLOUR}` field is packed.", None, id="colour-api"),
    pytest.param(f"The {API_COLOUR} field.", API_COLOUR, id="colour-prose"),
    pytest.param(
        f'`#[tokio::test({API_FLAVOUR} = "current_thread")]` runs one thread.',
        None,
        id="tokio-attribute",
    ),
    pytest.param(f"A different {API_FLAVOUR}.", API_FLAVOUR, id="tokio-prose"),
    pytest.param(
        f"`handle_post_article(path, title, flags, {API_FLAVOUR}, data)` posts.",
        None,
        id="hotline-signature",
    ),
    pytest.param(
        f"`handle_post_article(path, {API_FLAVOUR})` posts.",
        API_FLAVOUR,
        id="hotline-partial",
    ),
    pytest.param(
        f"`BackoffConfig::{API_NORMALIZED}` clamps.", None, id="wireframe-method"
    ),
    pytest.param(
        f"The delays are {API_NORMALIZED}.", API_NORMALIZED, id="wireframe-prose"
    ),
    pytest.param(
        f"`AppFactory<{API_SERVER}, Ctx, E, Codec>` builds apps.",
        None,
        id="wireframe-generic",
    ),
    pytest.param(
        f"`AppFactory<{API_SERVER}>` builds apps.", API_SERVER, id="generic-partial"
    ),
)


@pytest.mark.parametrize(("body", "flagged"), INLINE_CODE_CASES)
def test_an_inline_code_exception_covers_only_its_pattern(
    tracked_tree: cabc.Callable[..., Path], body: str, flagged: str | None
) -> None:
    """Each anchored exception admits its interface and nothing else.

    The second row of each pair is the narrowness half: the word the exception
    quotes is still checked outside the exact pattern, and the rejection must
    name that word so an unrelated failure cannot satisfy it.
    """
    result = _run_gate(tracked_tree(body))
    output = result.stdout + result.stderr
    if flagged is None:
        assert result.returncode == 0, f"{body!r} must pass; output={output!r}"
    else:
        assert result.returncode != 0, f"{body!r} must fail the gate"
        assert f"`{flagged}`" in output, f"the gate must name {flagged!r}: {output!r}"
