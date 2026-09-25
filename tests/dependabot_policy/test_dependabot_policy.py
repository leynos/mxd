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

import re
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


# Estate Dependabot policy (2026-09-24): every entry runs daily, one catch-all
# group per ecosystem batches minor and patch bumps, and majors stay ungrouped
# except for a named lockstep family, which must precede the catch-all because
# Dependabot assigns a dependency to the first group that matches it.
ECOSYSTEMS: typ.Final = ("github-actions", "cargo")
CATCH_ALL_UPDATE_TYPES: typ.Final = frozenset({"minor", "patch"})
# rstest-bdd and rstest-bdd-macros release together; Cargo counts a 0.x minor
# as a major, which the catch-all leaves ungrouped, so the pair would split.
LOCKSTEP_GROUPS: typ.Final = {"cargo": {"rstest-bdd": ["rstest-bdd*"]}}
ACTION_MANIFESTS: typ.Final = frozenset({"action.yml", "action.yaml"})


def _groups(entry: dict[str, object], ecosystem: str) -> dict[str, dict[str, object]]:
    """The groups an entry declares, in file order."""
    groups = entry.get("groups")
    assert isinstance(groups, dict) and groups, f"{ecosystem} must declare groups"
    assert all(isinstance(group, dict) for group in groups.values()), (
        f"every {ecosystem} group must be a mapping"
    )
    return typ.cast("dict[str, dict[str, object]]", groups)


def _is_catch_all(group: dict[str, object]) -> bool:
    """Whether a group matches every dependency but only minor and patch."""
    update_types = group.get("update-types")
    return (
        group.get("patterns") == ["*"]
        and isinstance(update_types, list)
        and frozenset(update_types) == CATCH_ALL_UPDATE_TYPES
    )


def directory_glob_matches(glob: str, directory: str) -> bool:
    """Whether a Dependabot directory glob covers a directory.

    `*` stays within one path segment and `**` spans segments, so
    `/.github/actions/*` covers `/.github/actions/setup-rust` and not
    `/.github/actions/release/sign`.
    """
    regex = "".join(
        ".*" if part == "**" else "[^/]*" if part == "*" else re.escape(part)
        for part in re.split(r"(\*\*|\*)", glob)
    )
    return re.fullmatch(regex, directory) is not None


def test_every_ecosystem_is_under_the_policy(configuration: dict[str, object]) -> None:
    """A new ecosystem joins the policy deliberately rather than bypassing it."""
    declared = sorted(
        str(entry.get("package-ecosystem")) for entry in _updates(configuration)
    )
    assert declared == sorted(ECOSYSTEMS), (
        f"the policy covers {sorted(ECOSYSTEMS)}; the file declares {declared}"
    )


@pytest.mark.parametrize("ecosystem", ECOSYSTEMS)
def test_every_entry_runs_daily(
    configuration: dict[str, object], ecosystem: str
) -> None:
    """Each ecosystem is checked for updates every day."""
    schedule = _entry(configuration, ecosystem).get("schedule")
    assert isinstance(schedule, dict) and schedule.get("interval") == "daily", (
        f"{ecosystem} must run daily; found {schedule!r}"
    )


@pytest.mark.parametrize("ecosystem", ECOSYSTEMS)
def test_each_entry_batches_minor_and_patch_and_leaves_majors_alone(
    configuration: dict[str, object], ecosystem: str
) -> None:
    """One trailing catch-all takes routine bumps; only lockstep groups take majors."""
    groups = _groups(_entry(configuration, ecosystem), ecosystem)
    catch_all_names = [name for name, group in groups.items() if _is_catch_all(group)]
    assert len(catch_all_names) == 1, (
        f"{ecosystem} needs exactly one catch-all limited to minor and patch; "
        f"found {catch_all_names}"
    )
    catch_all = groups[catch_all_names[0]]
    assert "exclude-patterns" not in catch_all, (
        f"the {ecosystem} catch-all must not exclude dependencies"
    )
    assert catch_all.get("applies-to", "version-updates") == "version-updates", (
        f"the {ecosystem} catch-all must apply to version updates"
    )
    assert list(groups)[-1] == catch_all_names[0], (
        f"the {ecosystem} catch-all must come last so lockstep groups claim "
        "their members first"
    )
    lockstep = LOCKSTEP_GROUPS.get(ecosystem, {})
    others = {
        name: group for name, group in groups.items() if name != catch_all_names[0]
    }
    assert sorted(others) == sorted(lockstep), (
        f"{ecosystem} may group majors only in {sorted(lockstep)}; found {sorted(others)}"
    )
    for name, patterns in lockstep.items():
        assert others[name] == {"patterns": patterns}, (
            f"the {ecosystem} {name} group must be exactly patterns {patterns}; "
            f"found {others[name]}"
        )


def test_github_actions_reaches_every_composite_action(
    configuration: dict[str, object],
) -> None:
    """Dependabot does not descend from `/` into `.github/actions`."""
    entry = _entry(configuration, "github-actions")
    globs = entry.get("directories") or [entry.get("directory")]
    assert isinstance(globs, list), "github-actions directories must be a list"
    actions = sorted(
        "/" + manifest.parent.relative_to(REPO_ROOT).as_posix()
        for manifest in (REPO_ROOT / ".github" / "actions").rglob("action.y*ml")
        if manifest.name in ACTION_MANIFESTS
    )
    assert actions, "expected composite actions under .github/actions"
    uncovered = [
        action
        for action in actions
        if not any(directory_glob_matches(str(glob), action) for glob in globs)
    ]
    assert not uncovered, (
        f"github-actions lists {globs}, which does not reach {uncovered}; "
        "their pins would drift behind the workflows"
    )


@pytest.mark.parametrize(
    ("glob", "directory", "expected"),
    [
        ("/", "/", True),
        ("/.github/actions/*", "/.github/actions/setup-rust", True),
        ("/.github/actions/*", "/.github/actions/release/sign", False),
        ("/.github/actions/**", "/.github/actions/release/sign", True),
    ],
)
def test_directory_globs_match_like_dependabot(
    glob: str, directory: str, *, expected: bool
) -> None:
    """`*` stays within one path segment; `**` spans segments."""
    assert directory_glob_matches(glob, directory) is expected, (
        f"{glob!r} should {'cover' if expected else 'not cover'} {directory!r}"
    )
