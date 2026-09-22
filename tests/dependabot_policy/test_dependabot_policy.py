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


def _updates() -> list[dict[str, object]]:
    """Every ecosystem entry in the Dependabot configuration."""
    document = yaml.safe_load(CONFIGURATION.read_text(encoding="utf-8"))
    updates = document.get("updates")
    assert isinstance(updates, list) and updates, (
        "this contract is meaningless if no update entry was read"
    )
    return [entry for entry in updates if isinstance(entry, dict)]


def _entry(ecosystem: str) -> dict[str, object]:
    """The one entry for an ecosystem.

    Exactly one is required rather than the first of several. Two entries for
    the same ecosystem would let a second, unasserted one govern a directory
    while this contract read the first and passed.
    """
    matching = [
        entry for entry in _updates() if entry.get("package-ecosystem") == ecosystem
    ]
    assert len(matching) == 1, (
        f"expected exactly one {ecosystem} entry, found {len(matching)}"
    )
    return matching[0]


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
