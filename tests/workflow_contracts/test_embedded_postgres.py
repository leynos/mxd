"""Every PostgreSQL test in CI runs against an embedded cluster, serialized.

The postgres leg once ran against a `postgres:15` service container reached
through `POSTGRES_TEST_URL`, so the embedded path the suite is written for
never ran in CI at all. Both are gone, and these contracts keep them gone:
no job declares a service container, no workflow sets the URL, and no Rust
source reads it.

The embedded path brings two obligations of its own, asserted here per job
that runs the PostgreSQL tests. Every cluster in a run shares one data
directory and pg-embed-setup-unpriv does not coordinate across the processes
nextest runs, so the tests run under the `postgres` nextest profile, whose
single-slot group serializes them. Each process also generates its own
superuser password unless `PG_PASSWORD` fixes one, and a cluster directory left
by one process then refuses the next, so each such job pins it. And a cluster
bootstrap failure is cached
for the rest of a test process, so the binaries are downloaded by
`make warm-postgres` before the tests start rather than by the first test.
"""

from __future__ import annotations

import json
import tomllib
import typing as typ

import pytest
from ci_workflow_reader import REPO_ROOT, repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

WARM_COMMAND: typ.Final = "make warm-postgres"
PROFILE: typ.Final = "postgres"
URL_VARIABLE: typ.Final = "POSTGRES_TEST_URL"


class PostgresTestStep(typ.NamedTuple):
    """A job that runs the PostgreSQL tests, and what its test step must set."""

    workflow: str
    job_id: str
    step_name: str
    profile: str


POSTGRES_TEST_STEPS: typ.Final = (
    PostgresTestStep(
        "ci.yml",
        "build-test",
        "Test",
        "${{ matrix.name == 'postgres' && 'postgres' || 'default' }}",
    ),
    PostgresTestStep("ci.yml", "coverage", "Generate coverage for Postgres", PROFILE),
    PostgresTestStep(
        "coverage-main.yml",
        "coverage-upload",
        "Generate coverage for Postgres",
        PROFILE,
    ),
)


@pytest.fixture(scope="module")
def documents() -> cabc.Mapping[str, cabc.Mapping[str, object]]:
    """This repository's parsed workflows."""
    return repository_documents()


def _jobs(document: cabc.Mapping[str, object]) -> dict[str, dict[str, object]]:
    """Every job in a workflow that parses to a mapping, by identifier."""
    declared = document.get("jobs")
    if not isinstance(declared, dict):
        return {}
    return {name: job for name, job in declared.items() if isinstance(job, dict)}


def _steps(job: cabc.Mapping[str, object]) -> list[dict[str, object]]:
    """A job's steps that parse to mappings, in order."""
    declared = job.get("steps")
    if not isinstance(declared, list):
        return []
    return [step for step in declared if isinstance(step, dict)]


def _text(document: cabc.Mapping[str, object]) -> str:
    """Every key and value of a parsed document, as one searchable text."""
    return json.dumps(document, default=str)


def test_no_job_declares_a_service_container(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """A service is how an external database would come back."""
    offenders = [
        f"{workflow}:{name}"
        for workflow, document in documents.items()
        for name, job in _jobs(document).items()
        if "services" in job
    ]
    assert not offenders, f"these jobs declare services: {offenders}"


def test_no_workflow_mentions_the_external_url(
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """Read from every key and value, so an env block or a run body counts."""
    offenders = [
        workflow
        for workflow, document in documents.items()
        if URL_VARIABLE in _text(document)
    ]
    assert not offenders, f"{URL_VARIABLE} appears in {offenders}"


def test_no_rust_source_reads_the_external_url() -> None:
    """The suite has no route to an external server to be switched on."""
    roots = ("src", "tests", "test-util", "validator")
    offenders = sorted(
        str(path.relative_to(REPO_ROOT))
        for root in roots
        for path in (REPO_ROOT / root).rglob("*.rs")
        if URL_VARIABLE in path.read_text(encoding="utf-8")
    )
    assert not offenders, f"{URL_VARIABLE} is read in {offenders}"


@pytest.mark.parametrize(
    "expected",
    POSTGRES_TEST_STEPS,
    ids=[f"{step.workflow}:{step.job_id}" for step in POSTGRES_TEST_STEPS],
)
def test_the_postgres_tests_run_serialized_after_the_download(
    expected: PostgresTestStep,
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """The password is pinned, the profile set, and the warm-up run first."""
    where = f"{expected.workflow}:{expected.job_id}"
    job = _jobs(documents[expected.workflow])[expected.job_id]
    job_env = job.get("env")
    assert isinstance(job_env, dict) and job_env.get("PG_PASSWORD"), (
        f"{where} must pin PG_PASSWORD, or a cluster directory left by one test "
        "process refuses the next"
    )
    steps = _steps(job)
    positions = [
        i for i, step in enumerate(steps) if step.get("name") == expected.step_name
    ]
    assert len(positions) == 1, f"expected one {expected.step_name!r} step in {where}"
    (test_at,) = positions
    env = steps[test_at].get("env")
    assert isinstance(env, dict) and env.get("NEXTEST_PROFILE") == expected.profile, (
        f"{where} {expected.step_name!r} must set NEXTEST_PROFILE to "
        f"{expected.profile!r}"
    )
    warm_at = [i for i, step in enumerate(steps) if step.get("run") == WARM_COMMAND]
    assert len(warm_at) == 1 and warm_at[0] < test_at, (
        f"{where} must run {WARM_COMMAND!r} once, before {expected.step_name!r}"
    )


def test_the_postgres_profile_serializes_every_test() -> None:
    """One slot, applied to every test the profile runs."""
    config = tomllib.loads(
        (REPO_ROOT / ".config" / "nextest.toml").read_text(encoding="utf-8")
    )
    group = config["test-groups"]["pg-embed"]
    assert group == {"max-threads": 1}, f"pg-embed group is {group}"
    overrides = config["profile"][PROFILE]["overrides"]
    assert {"filter": "all()", "test-group": "pg-embed"} in overrides, overrides
