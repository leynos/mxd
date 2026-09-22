"""Reading workflows for the CodeScene boundary contract.

Every function here takes parsed documents or source text rather than reading
this repository itself, so the readers can be driven with constructed
workflows. That matters because the repository's own workflows declare only
the shapes the contract accepts: a reader proved only against them would pass
with every refusal deleted.
"""

from __future__ import annotations

import re
import typing as typ

import yaml

if typ.TYPE_CHECKING:
    import collections.abc as cabc

# Where this repository's workflows live, as a `uses:` value names them.
WORKFLOW_PREFIX: typ.Final = ".github/workflows/"

# The two spellings GitHub documents for a call into this repository's own
# workflows, `./` and `$/`. The second is the recommendation on github.com.
LOCAL_PREFIX: typ.Final = re.compile(r"^(?:\./|\$/)")

# The events that put a workflow on the pull-request surface. Both run for a
# pull request, and `pull_request_target` runs with the base repository's
# secrets, so leaving it out would exempt the more dangerous of the two.
PULL_REQUEST_EVENTS: typ.Final = ("pull_request", "pull_request_target")

# What a pull-request lane may not name, reach, or contact.
CODESCENE_SECRET: typ.Final = "CS_ACCESS_TOKEN"
CODESCENE_HOST: typ.Final = "codescene.io"

# An expression reading the secrets context as a whole rather than by name.
# `toJSON(secrets)` and `secrets[...]` hand over the token without naming it.
WHOLE_SECRETS: typ.Final = re.compile(r"tojson\(\s*secrets\s*\)|secrets\s*\[")

MERGE_TAG: typ.Final = "tag:yaml.org,2002:merge"

# GitHub reads both extensions, in any case, from the workflows directory.
WORKFLOW_SUFFIXES: typ.Final = (".yml", ".yaml")


class _StrictLoader(yaml.SafeLoader):
    """A safe loader that refuses a mapping declaring the same key twice."""

    def construct_mapping(
        self, node: yaml.MappingNode, deep: bool = False
    ) -> dict[object, object]:
        """Refuse a duplicate key before PyYAML silently keeps the last one.

        A workflow declaring `runs-on` or `uses` twice parses into a document
        that discarded the first value, so the contract would judge a file
        GitHub reads differently. Merge keys are skipped, since overriding a
        merged value is what they are for.
        """
        seen: set[object] = set()
        for key_node, _ in node.value:
            if key_node.tag == MERGE_TAG:
                continue
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


def is_workflow_file(name: str) -> bool:
    """Whether GitHub would read a file of this name as a workflow.

    A reader globbing `*.yml` skips `release.yaml` and `CI.YML` in silence,
    and a workflow it never reads is a workflow every prohibition passes over.

    >>> is_workflow_file("Release.YAML")
    True
    """
    return name.lower().endswith(WORKFLOW_SUFFIXES)


def load(source: str) -> dict[str, object]:
    """Parse a workflow, refusing duplicate keys and non-mapping documents.

    >>> load("on: push\\njobs: {}\\n")[True]
    'push'
    """
    document = yaml.load(source, Loader=_StrictLoader)
    if not isinstance(document, dict):
        message = f"a workflow must be a mapping, not {type(document).__name__}"
        raise TypeError(message)
    return document


def triggers(workflow: dict[str, object]) -> dict[str, object]:
    """A workflow's `on:` block, as a mapping of event name to configuration.

    YAML resolves the bare key `on` to the boolean `True`, so a reader keyed on
    the string finds nothing and every workflow reads as triggered by nothing,
    which would make every assertion vacuously true.

    All three forms GitHub accepts are normalized. `on: push` is a string and
    `on: [push, pull_request]` is a list, and both are as valid as the mapping
    form. Stringifying the list produced one key named
    `"['push', 'pull_request']"`, so a workflow written that way escaped every
    prohibition. An unsupported shape is refused rather than coerced.

    >>> triggers({True: ["push", "pull_request"]})
    {'push': None, 'pull_request': None}
    """
    for key in (True, "on"):
        if key not in workflow:
            continue
        match workflow[key]:
            case dict() as mapping:
                return mapping
            case str() as event:
                return {event: None}
            case list() as events if all(isinstance(e, str) for e in events):
                return dict.fromkeys(typ.cast("list[str]", events))
            case other:
                message = f"unsupported `on:` shape {other!r}"
                raise AssertionError(message)
    message = "a workflow with no `on:` block cannot be classified"
    raise AssertionError(message)


def jobs(document: dict[str, object]) -> list[dict[str, object]]:
    """Every job a workflow declares that is a mapping."""
    declared = document.get("jobs")
    if not isinstance(declared, dict):
        return []
    return [job for job in declared.values() if isinstance(job, dict)]


def steps(document: dict[str, object]) -> list[dict[str, object]]:
    """Every step of every job, plus each job that is itself a call.

    A job calling a reusable workflow has no steps and carries its `uses` on
    the job, so a walker that descended only into `steps` would miss the one
    shape that can run another repository's code.
    """
    found: list[dict[str, object]] = []
    for job in jobs(document):
        if "uses" in job:
            found.append(job)
        declared = job.get("steps")
        if isinstance(declared, list):
            found.extend(step for step in declared if isinstance(step, dict))
    return found


def local_call(used: str) -> str | None:
    """The workflow a `uses:` value names in this repository, or `None`.

    Matched by shape: a leading `./` or `$/` is stripped and what remains is
    asked whether it is a path under this repository's workflow directory. A
    call to another repository carries an `owner/repo/` prefix and an `@ref`,
    so it cannot reach this directory.

    >>> local_call("$/.github/workflows/release.yml")
    'release.yml'
    """
    candidate = LOCAL_PREFIX.sub("", used.strip(), count=1)
    if not candidate.startswith(WORKFLOW_PREFIX) or "@" in candidate:
        return None
    name = candidate.removeprefix(WORKFLOW_PREFIX)
    return name or None


def called_locally(document: dict[str, object]) -> list[str]:
    """The workflows a workflow calls from this repository."""
    return [
        name
        for step in steps(document)
        if (name := local_call(str(step.get("uses", ""))))
    ]


def _closure(
    documents: dict[str, dict[str, object]], entries: cabc.Iterable[str]
) -> set[str]:
    """The entry workflows and everything they call locally, transitively.

    A seen set stops a cycle hanging the collection.
    """
    pending = list(entries)
    reached: set[str] = set()
    while pending:
        name = pending.pop()
        if name in reached or name not in documents:
            continue
        reached.add(name)
        pending.extend(called_locally(documents[name]))
    return reached


def _workflow_run_sources(document: dict[str, object]) -> set[str]:
    """The workflow names a `workflow_run` trigger waits on."""
    declared = triggers(document).get("workflow_run")
    waits_on = declared.get("workflows") if isinstance(declared, dict) else None
    match waits_on:
        case str():
            return {waits_on}
        case list():
            return {entry for entry in waits_on if isinstance(entry, str)}
        case _:
            return set()


def _chained_after(
    documents: dict[str, dict[str, object]], reached: set[str]
) -> list[str]:
    """Workflows outside the surface that a `workflow_run` chains onto it.

    `workflow_run` names the workflows it waits on by their `name:`, which
    defaults to the file's path, so both are matched.
    """
    names = {str(documents[name].get("name", name)) for name in reached}
    names |= {f"{WORKFLOW_PREFIX}{name}" for name in reached}
    return [
        name
        for name, document in documents.items()
        if name not in reached and _workflow_run_sources(document) & names
    ]


def pull_request_surface(documents: dict[str, dict[str, object]]) -> tuple[str, ...]:
    """Every workflow a pull request can run, calls and chains included.

    A workflow triggered by a pull request is only the entry point. A job that
    calls a local reusable workflow runs that workflow's jobs under the
    caller's trigger, so a workflow declaring only `workflow_call` is on the
    surface when a pull-request lane calls it. A `workflow_run` workflow
    waiting on a surface workflow runs after every pull request, with the base
    repository's secrets, so it joins too; one chained only onto push or
    schedule lanes does not. Repeated until nothing new joins.
    """
    reached = _closure(
        documents,
        (
            name
            for name, document in documents.items()
            if any(event in triggers(document) for event in PULL_REQUEST_EVENTS)
        ),
    )
    while chained := _chained_after(documents, reached):
        reached |= _closure(documents, chained)
    return tuple(sorted(reached))


def _scalars(node: object) -> cabc.Iterator[str]:
    """Every key and scalar value in a parsed document, as text."""
    match node:
        case dict():
            for key, value in node.items():
                yield str(key)
                yield from _scalars(value)
        case list():
            for item in node:
                yield from _scalars(item)
        case _:
            yield str(node)


def _inherits_to_another_repository(job: dict[str, object]) -> bool:
    """Whether a job hands every secret to a workflow this tree cannot read."""
    inherits = str(job.get("secrets", "")).strip() == "inherit"
    return inherits and local_call(str(job.get("uses", ""))) is None


def secret_breaches(document: dict[str, object]) -> list[str]:
    """How a pull-request workflow could reach CodeScene or its token.

    The token and the host are searched for in every key and scalar of the
    parsed document. The parser has already dropped real comments, and a line
    starting `#` inside a block scalar is data, which Actions still expands, so
    stripping such lines from the source would hide an expression. A secret
    reaches a step through `env` at any scope, through
    `with`, through a named `secrets:` forward, or through an expression in a
    `run` body, and a reader walking one of those routes would pass on the
    others. Two routes name nothing, so they are read separately: an
    expression over the whole secrets context, and `secrets: inherit` on a
    call to another repository. Inheriting into a local call is allowed,
    because the callee is on the surface and read in turn.
    """
    text = "\n".join(_scalars(document)).lower()
    found: list[str] = []
    if CODESCENE_SECRET.lower() in text:
        found.append(f"names {CODESCENE_SECRET}")
    if CODESCENE_HOST in text:
        found.append(f"contacts {CODESCENE_HOST}")
    if WHOLE_SECRETS.search(text):
        found.append("reads the whole secrets context")
    found.extend(
        f"inherits every secret into {job.get('uses')!r}"
        for job in jobs(document)
        if _inherits_to_another_repository(job)
    )
    return found
