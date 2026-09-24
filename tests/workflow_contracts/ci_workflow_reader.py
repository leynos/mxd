"""Parsing primitives for this repository's GitHub Actions workflows.

The CI contracts assert what would break a gate rather than what its author
meant, so they need each workflow's structure rather than its text. This module
owns the loading half: locating workflow files, parsing them, reading the
trigger mapping, and narrowing individual fields.

This module is the only place that touches the filesystem or the YAML parser,
and :func:`repository_documents` is the only function anywhere that reads this
repository without being told to. Every other function here requires the
directory or the path it should read, and the record and judgement modules take
parsed documents and never load anything themselves. A default directory on any
of them would be a second implicit route to the same files, which is the thing
this boundary exists to prevent. Failures are typed rather than raw:
a file that cannot be read, or text that is not YAML, raises
:class:`WorkflowLoadError` naming the path and the operation and chaining the
original exception, and a shape this reader deliberately does not model raises
:class:`WorkflowShapeError` rather than being quietly dropped.

Job and step records are built in :mod:`ci_workflow_jobs`. See
``docs/developers-guide.md``, "The workflow contracts", for the contracts
themselves.

The module is a deliberate copy of the one dev-env-rocky carries rather than a
shared package. A shared package would make every repository's contracts wait
on a release, and the estate's rule is that a contract is chosen from the
change in front of it.
"""

from __future__ import annotations

import typing as typ
from pathlib import Path

import yaml

if typ.TYPE_CHECKING:
    import collections.abc as cabc

REPO_ROOT: typ.Final = Path(__file__).resolve().parents[2]
WORKFLOW_DIRECTORY: typ.Final = REPO_ROOT / ".github" / "workflows"
# PyYAML follows YAML 1.1, in which the bare key ``on`` is the boolean true,
# while a quoted ``'on'`` stays a string. GitHub accepts both spellings, so
# every trigger lookup reads both.
TRIGGER_KEYS: typ.Final = (True, "on")
WORKFLOW_SUFFIXES: typ.Final = (".yml", ".yaml")


class WorkflowReaderError(RuntimeError):
    """Base class for every failure this reader reports.

    A caller that wants to treat any reader failure alike catches this; a
    caller that distinguishes a file it could not read from a construct it
    will not model catches one of the two subclasses.
    """


class WorkflowLoadError(WorkflowReaderError):
    """A workflow file could not be read or could not be parsed as YAML.

    Raised in place of the underlying :class:`OSError` or
    :class:`yaml.YAMLError` so that the loader's failure model is the one its
    documentation names. The original exception is chained, and the message
    names both the path and the operation that failed, because "file not
    found" without a path is indistinguishable from a bug in the reader.
    """


class _StrictLoader(yaml.SafeLoader):
    """A safe loader that refuses a mapping declaring the same key twice."""

    def construct_mapping(
        self, node: yaml.MappingNode, deep: bool = False
    ) -> dict[object, object]:
        """Refuse a duplicate key rather than keep the last value silently.

        PyYAML keeps the last of two equal keys, so a workflow repeating
        ``jobs``, ``runs-on`` or ``with`` would be judged on a document that
        discarded the first declaration. The refusal is a
        :class:`yaml.constructor.ConstructorError`, a :class:`yaml.YAMLError`,
        so :func:`load_workflow` reports it as a :class:`WorkflowLoadError`.
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


class WorkflowShapeError(WorkflowReaderError):
    """A workflow uses a shape this reader deliberately does not model.

    Raised instead of quietly skipping the construct, because a contract that
    silently examines less of the tree than it claims is worse than no contract
    at all.
    """


def workflow_paths(directory: Path) -> tuple[Path, ...]:
    """Collect every workflow file, covering both YAML suffixes.

    A contract that reads only ``*.yml`` stops seeing a lane the moment
    somebody renames it, so both suffixes are collected here once, in any
    case, since ``glob`` is case-sensitive here and ``CI.YML`` is a workflow.

    Parameters
    ----------
    directory
        The directory to read. Required: :func:`repository_documents` is the
        only function that may reach this repository's workflows without being
        told to, so every other caller names the directory it means.

    Returns
    -------
    tuple[Path, ...]
        Workflow file paths in sorted order.

    Raises
    ------
    WorkflowLoadError
        When the directory cannot be listed.
    WorkflowShapeError
        When the directory holds no workflow at all.
    """
    try:
        paths = sorted(
            path
            for path in directory.iterdir()
            if path.suffix.lower() in WORKFLOW_SUFFIXES and path.is_file()
        )
    except OSError as error:
        message = f"listing workflows under {directory} failed: {error}"
        raise WorkflowLoadError(message) from error
    if not paths:
        message = f"no workflows found under {directory}"
        raise WorkflowShapeError(message)
    return tuple(paths)


def load_workflow(path: Path) -> cabc.Mapping[str, object]:
    """Parse one workflow file.

    Parameters
    ----------
    path
        The workflow file to read.

    Returns
    -------
    cabc.Mapping[str, object]
        The parsed workflow document.

    Raises
    ------
    WorkflowLoadError
        When the file cannot be read, or its text is not valid YAML.
    WorkflowShapeError
        When the file does not parse to a mapping.
    """
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        message = f"reading {path} failed: {error}"
        raise WorkflowLoadError(message) from error
    try:
        document = yaml.load(text, Loader=_StrictLoader)
    except yaml.YAMLError as error:
        message = f"parsing {path} as YAML failed: {error}"
        raise WorkflowLoadError(message) from error
    if not isinstance(document, dict):
        message = f"{path.name} must parse to a mapping"
        raise WorkflowShapeError(message)
    return typ.cast("cabc.Mapping[str, object]", document)


def workflow_documents(
    directory: Path,
) -> cabc.Mapping[str, cabc.Mapping[str, object]]:
    """Parse every workflow in a directory.

    Parameters
    ----------
    directory
        The directory to read. Required, for the reason given on
        :func:`workflow_paths`.

    Returns
    -------
    cabc.Mapping[str, cabc.Mapping[str, object]]
        Each parsed workflow, keyed by file name.

    Notes
    -----
    Propagates :class:`WorkflowLoadError` from :func:`workflow_paths` and
    :func:`load_workflow` when a workflow cannot be listed, read or parsed,
    and :class:`WorkflowShapeError` when the directory holds no workflow or
    one does not parse to a mapping.
    """
    return {path.name: load_workflow(path) for path in workflow_paths(directory)}


def repository_documents() -> cabc.Mapping[str, cabc.Mapping[str, object]]:
    """Parse this repository's own workflows.

    This is the only entry point, here or anywhere, that reads the repository
    without being told to. Contracts call it explicitly and pass the result to
    the record and query functions, so that no query loads a file as a side
    effect of being called with its arguments omitted. Each call parses afresh
    and nothing is cached: a cached mapping is shared and mutable, so one
    caller editing it would change what every later caller reads. Contracts
    load it once at their fixture boundary instead.

    Returns
    -------
    cabc.Mapping[str, cabc.Mapping[str, object]]
        Each parsed workflow, keyed by file name.

    Notes
    -----
    Propagates the same failures as :func:`workflow_documents`.
    """
    return workflow_documents(WORKFLOW_DIRECTORY)


def triggers(workflow: cabc.Mapping[str, object]) -> cabc.Mapping[str, object]:
    """Read a workflow's triggers as a mapping of event to configuration.

    ``pull_request:`` with no filters parses to ``None``, which is the value a
    trigger contract needs: a membership test passes just as happily when a
    ``branches`` or ``paths`` filter excludes every pull request the repository
    opens, so the absence of filters has to be readable.

    The key is read under both spellings, since a quoted ``'on':`` parses to
    the string and a bare ``on:`` to the boolean, and a reader keyed on one
    reads the other as triggered by nothing. All three forms GitHub accepts
    are normalized: ``on: push`` and ``on: [push, pull_request]`` name events
    exactly as the mapping form does.

    Parameters
    ----------
    workflow
        A parsed workflow document.

    Returns
    -------
    cabc.Mapping[str, object]
        The workflow's declared triggers.

    Raises
    ------
    WorkflowShapeError
        When the workflow declares no triggers, declares them under both
        spellings of the key, or in a shape other than a mapping, an event
        name or a list of event names.
    """
    declared = [key for key in TRIGGER_KEYS if key in workflow]
    if len(declared) != 1:
        # Both spellings at once is refused rather than resolved: GitHub
        # merges them, and a reader that picked one would be blind to the
        # events under the other.
        message = f"workflow must declare `on:` exactly once, found {len(declared)}"
        raise WorkflowShapeError(message)
    match workflow[declared[0]]:
        case dict() as mapping:
            return typ.cast("cabc.Mapping[str, object]", mapping)
        case str() as event:
            return {event: None}
        case list() as events if all(isinstance(event, str) for event in events):
            return dict.fromkeys(typ.cast("list[str]", events))
        case other:
            message = f"unsupported `on:` shape {other!r}"
            raise WorkflowShapeError(message)


def _narrowed(
    value: object,
    expected: type,
    description: str,
    coordinate: str,
) -> object | None:
    """Narrow an optional field to one type, or refuse it.

    Both public narrowing helpers differ only in the type they accept and the
    word they use for it, so the absent case, the refusal and the wording of
    the message live here once.

    Parameters
    ----------
    value
        The parsed field value.
    expected
        The type the field must have when it is present.
    description
        The type as the error message should word it, such as ``"a string"``.
    coordinate
        ``workflow:job`` text naming the location, used in any error raised.

    Returns
    -------
    object | None
        The value, or ``None`` when the key is absent.

    Raises
    ------
    WorkflowShapeError
        When the field is present but has the wrong type.
    """
    if value is None:
        return None
    if not isinstance(value, expected):
        message = f"{coordinate}: expected {description}, got {value!r}"
        raise WorkflowShapeError(message)
    return value


def optional_text(value: object, coordinate: str) -> str | None:
    """Narrow a string field that may be absent.

    Parameters
    ----------
    value
        The parsed field value.
    coordinate
        ``workflow:job`` text naming the location, used in any error raised.

    Returns
    -------
    str | None
        The field's text, or ``None`` when the key is absent.

    Notes
    -----
    Propagates :class:`WorkflowShapeError` when the field is present but is
    not a string.
    """
    return typ.cast("str | None", _narrowed(value, str, "a string", coordinate))


def sub_mapping(value: object, coordinate: str) -> cabc.Mapping[str, object]:
    """Narrow a mapping field that may be absent.

    Parameters
    ----------
    value
        The parsed field value.
    coordinate
        ``workflow:job`` text naming the location, used in any error raised.

    Returns
    -------
    cabc.Mapping[str, object]
        The mapping, or an empty mapping when the key is absent.

    Notes
    -----
    Propagates :class:`WorkflowShapeError` when the field is present but is
    not a mapping.
    """
    narrowed = _narrowed(value, dict, "a mapping", coordinate)
    if narrowed is None:
        return {}
    return typ.cast("cabc.Mapping[str, object]", narrowed)
