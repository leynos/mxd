"""Dependabot raises the manifest for Cargo, asserted against its configuration.

A lockfile-only bump across a major boundary the manifest forbids is not a
change: the next command that writes `Cargo.lock` resolves the edge back, so the
pull request merges with no net effect and the same bump is proposed again. It
has happened eight times here, and each round leaves `main` carrying a lockfile
nothing builds from with `--locked` until somebody runs cargo.

`versioning-strategy: increase-if-necessary` is what stops it, by letting
Dependabot raise `Cargo.toml` when the new version needs it. This asserts the
key is set, on the Cargo entry specifically, and refuses the two values that
would bring the churn back.

The complement is `make check-locked`, which refuses a lockfile the manifest
does not admit. That gate catches the defect after it lands; this setting stops
it being proposed.
"""

from __future__ import annotations

import typing as typ
from pathlib import Path

import pytest
import yaml

CONFIGURATION: typ.Final = (
    Path(__file__).resolve().parents[2] / ".github" / "dependabot.yml"
)
REQUIRED_STRATEGY: typ.Final = "increase-if-necessary"
# `lockfile-only` is the value that reproduces the defect exactly, and
# `auto` leaves the choice to Dependabot rather than stating it here.
REFUSED_STRATEGIES: typ.Final = ("lockfile-only", "auto")


class _StrictLoader(yaml.SafeLoader):
    """A safe loader that refuses a mapping declaring the same key twice."""

    def construct_mapping(
        self, node: yaml.MappingNode, deep: bool = False
    ) -> dict[object, object]:
        """Refuse a duplicate key rather than keep the last value silently.

        Dependabot parses this file with a loader that keeps the last of two
        equal keys too, so a repeated `updates` or `versioning-strategy` would
        be read as the later declaration while the earlier one sat in the file
        looking authoritative.
        """
        seen: set[object] = set()
        for key_node, _ in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in seen:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    f"found duplicate key {key!r}",
                    key_node.start_mark,
                )
            seen.add(key)
        return super().construct_mapping(node, deep=deep)


def _load(text: str) -> dict[str, object]:
    """Parse a Dependabot configuration, refusing duplicate keys."""
    document = yaml.load(text, Loader=_StrictLoader)
    assert isinstance(document, dict), "the configuration must be a mapping"
    return document


def _updates() -> list[dict[str, object]]:
    """Every ecosystem entry in the Dependabot configuration."""
    updates = _load(CONFIGURATION.read_text(encoding="utf-8")).get("updates")
    assert isinstance(updates, list) and updates, (
        "this contract is meaningless if no update entry was read"
    )
    return [entry for entry in updates if isinstance(entry, dict)]


def _entry(ecosystem: str) -> dict[str, object]:
    """The one entry for an ecosystem."""
    # Exactly one rather than the first of several: a second, unasserted
    # entry could govern a directory while this contract read the first.
    matching = [
        entry for entry in _updates() if entry.get("package-ecosystem") == ecosystem
    ]
    assert len(matching) == 1, (
        f"expected exactly one {ecosystem} entry, found {len(matching)}"
    )
    return matching[0]


@pytest.mark.parametrize(
    "text",
    [
        pytest.param("version: 2\nupdates: []\nupdates: []\n", id="updates"),
        pytest.param(
            "version: 2\nupdates:\n  - package-ecosystem: cargo\n"
            "    versioning-strategy: lockfile-only\n"
            "    versioning-strategy: increase-if-necessary\n",
            id="versioning-strategy",
        ),
    ],
)
def test_a_duplicate_key_is_refused(text: str) -> None:
    """A repeated key fails the load instead of resolving to its last value.

    The second case is the one that matters: read leniently, it passes the
    assertions below while the file also declares the refused strategy.
    """
    with pytest.raises(yaml.constructor.ConstructorError, match="duplicate key"):
        _load(text)


def test_the_cargo_entry_raises_the_manifest() -> None:
    """The setting that stops a lockfile-only major bump being proposed."""
    assert _entry("cargo").get("versioning-strategy") == REQUIRED_STRATEGY, (
        f"the cargo entry must set versioning-strategy: {REQUIRED_STRATEGY}, or "
        "Dependabot proposes lockfile pins the manifest forbids"
    )


@pytest.mark.parametrize("refused", REFUSED_STRATEGIES)
def test_the_cargo_entry_refuses_a_strategy_that_restores_the_churn(
    refused: str,
) -> None:
    """Named values, not merely "something other than the required one".

    `lockfile-only` reproduces the defect exactly. `auto` hands the decision
    back to Dependabot, which is how the repository arrived here: the key was
    absent and the default behaved as `auto` does.
    """
    assert _entry("cargo").get("versioning-strategy") != refused, (
        f"versioning-strategy: {refused} restores the lockfile-only churn"
    )


def test_the_github_actions_entry_is_left_alone() -> None:
    """The narrowness half.

    Setting the strategy across every ecosystem would satisfy the assertions
    above while changing behaviour nobody asked about. Actions pins carry no
    manifest to raise, so the key means nothing there and its absence is the
    state this asserts.
    """
    assert "versioning-strategy" not in _entry("github-actions"), (
        "the github-actions entry has no manifest to raise; leave it unset"
    )
