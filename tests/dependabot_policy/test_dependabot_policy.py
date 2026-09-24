"""What the Dependabot configuration must hold, asserted against the file.

serial_test 4 declares `rust-version = "1.93.1"`, newer than the pinned
nightly, so Cargo resolves any bump to 4 back to 3.x. Dependabot does not read
`rust-version`: it proposed that lockfile-only bump twice, and automerge
landed the second while `make check-locked` refused the lockfile. The Cargo
entry therefore ignores serial_test at `>= 4`, and this contract holds the
rule and the toolchain pin it depends on.

It also holds the one shape Dependabot rejects outright. For Cargo,
`versioning-strategy` accepts only `lockfile-only` and `auto`; any other value
makes the whole file invalid, and every ecosystem in it stops updating.
"""

from __future__ import annotations

import typing as typ
from pathlib import Path

import pytest
import yaml

REPO_ROOT: typ.Final = Path(__file__).resolve().parents[2]
CONFIGURATION: typ.Final = REPO_ROOT / ".github" / "dependabot.yml"
TOOLCHAIN: typ.Final = REPO_ROOT / "rust-toolchain.toml"
# serial_test 4 declares rust-version 1.93.1; this nightly is rustc
# 1.93.0-nightly, so Cargo resolves any 4.x bump back to 3.x.
TOOLCHAIN_BELOW_SERIAL_TEST_4: typ.Final = 'channel = "nightly-2025-11-08"'
SERIAL_TEST_IGNORE: typ.Final = {"dependency-name": "serial_test", "versions": [">= 4"]}
# The only values Dependabot accepts for a Cargo entry's versioning-strategy.
# `increase-if-necessary`, valid for other ecosystems, fails the file's schema.
CARGO_STRATEGIES: typ.Final = frozenset({"lockfile-only", "auto"})


class _StrictLoader(yaml.SafeLoader):
    """A safe loader that refuses a mapping declaring the same key twice."""

    def construct_mapping(
        self, node: yaml.MappingNode, deep: bool = False
    ) -> dict[object, object]:
        """Refuse a duplicate key rather than keep the last value silently.

        Dependabot parses this file with a loader that keeps the last of two
        equal keys too, so a repeated `updates` or `ignore` would be read as
        the later declaration while the earlier one sat in the file looking
        authoritative.
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


class ConfigurationError(RuntimeError):
    """A file this contract reads could not be read or parsed."""


def _load(text: str) -> object:
    """Parse YAML text, refusing duplicate keys.

    Raises
    ------
    yaml.YAMLError
        When the text is not YAML, or repeats a key.
    """
    return yaml.load(text, Loader=_StrictLoader)


def read_text(path: Path) -> str:
    """Read one file this contract depends on.

    Raises
    ------
    ConfigurationError
        When the file cannot be read, naming the path.
    """
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        message = f"reading {path} failed: {error}"
        raise ConfigurationError(message) from error


def read_configuration(path: Path) -> dict[str, object]:
    """Read and parse a Dependabot configuration.

    Raises
    ------
    ConfigurationError
        When the file cannot be read, is not YAML, repeats a key, or does not
        parse to a mapping.
    """
    try:
        document = _load(read_text(path))
    except yaml.YAMLError as error:
        message = f"parsing {path} failed: {error}"
        raise ConfigurationError(message) from error
    if not isinstance(document, dict):
        message = f"{path} must parse to a mapping"
        raise ConfigurationError(message)
    return document


@pytest.fixture(scope="module")
def configuration() -> dict[str, object]:
    """This repository's Dependabot configuration, read once."""
    return read_configuration(CONFIGURATION)


def _updates(configuration: dict[str, object]) -> list[dict[str, object]]:
    """Every ecosystem entry in a parsed configuration."""
    updates = configuration.get("updates")
    assert isinstance(updates, list) and updates, (
        "this contract is meaningless if no update entry was read"
    )
    return [entry for entry in updates if isinstance(entry, dict)]


def _entry(configuration: dict[str, object], ecosystem: str) -> dict[str, object]:
    """The one entry for an ecosystem."""
    # Exactly one rather than the first of several: a second, unasserted
    # entry could govern a directory while this contract read the first.
    matching = [
        entry
        for entry in _updates(configuration)
        if entry.get("package-ecosystem") == ecosystem
    ]
    assert len(matching) == 1, (
        f"expected exactly one {ecosystem} entry, found {len(matching)}"
    )
    return matching[0]


def test_an_unreadable_configuration_is_a_named_error(tmp_path: Path) -> None:
    """A missing file and malformed YAML each name the path they concern."""
    missing = tmp_path / "dependabot.yml"
    with pytest.raises(ConfigurationError, match="reading .*dependabot.yml"):
        read_configuration(missing)
    missing.write_text("updates: [\n", encoding="utf-8")
    with pytest.raises(ConfigurationError, match="parsing .*dependabot.yml"):
        read_configuration(missing)


@pytest.mark.parametrize(
    "text",
    [
        pytest.param("version: 2\nupdates: []\nupdates: []\n", id="updates"),
        pytest.param(
            "version: 2\nupdates:\n  - package-ecosystem: cargo\n"
            "    ignore: []\n"
            "    ignore:\n      - dependency-name: serial_test\n",
            id="ignore",
        ),
    ],
)
def test_a_duplicate_key_is_refused(text: str) -> None:
    """A repeated key fails the load instead of resolving to its last value.

    The second case is the one that matters: read leniently, it passes the
    ignore assertion below while the file also declares an empty `ignore`.
    """
    with pytest.raises(yaml.constructor.ConstructorError, match="duplicate key"):
        _load(text)


def test_a_cargo_versioning_strategy_is_one_dependabot_accepts(
    configuration: dict[str, object],
) -> None:
    """An invalid value would disable Dependabot for the whole repository.

    The key may be absent, which means `auto`. When present it must be one of
    the two values the Cargo updater accepts.
    """
    strategy = _entry(configuration, "cargo").get("versioning-strategy")
    assert strategy is None or strategy in CARGO_STRATEGIES, (
        f"versioning-strategy {strategy!r} is not valid for cargo; Dependabot "
        f"accepts only {sorted(CARGO_STRATEGIES)} and rejects the whole file"
    )


def test_the_cargo_entry_ignores_serial_test_4(
    configuration: dict[str, object],
) -> None:
    """A serial_test 4 bump is never proposed while the toolchain predates it.

    Dependabot does not read `rust-version`, so it proposed the lockfile-only
    bump twice (#566, #575) and automerge landed a lockfile that
    `make check-locked` refuses.
    """
    ignored = _entry(configuration, "cargo").get("ignore")
    assert isinstance(ignored, list), "the cargo entry must declare ignore rules"
    matching = [
        rule
        for rule in ignored
        if isinstance(rule, dict) and rule.get("dependency-name") == "serial_test"
    ]
    assert matching == [SERIAL_TEST_IGNORE], (
        f"expected exactly {SERIAL_TEST_IGNORE}, found {matching}"
    )


def test_the_serial_test_ignore_still_has_its_reason() -> None:
    """The ignore is tied to the toolchain that needs it.

    When the pin moves, this fails so the ignore is reconsidered rather than
    left holding serial_test back after the reason has gone.
    """
    assert TOOLCHAIN_BELOW_SERIAL_TEST_4 in read_text(TOOLCHAIN), (
        "the toolchain pin moved: if it now reaches rustc 1.93.1, drop the "
        "serial_test ignore from .github/dependabot.yml and this case"
    )
