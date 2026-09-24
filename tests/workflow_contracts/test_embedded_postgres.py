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
single-slot group serializes them. And a cluster bootstrap failure is cached
for the rest of a test process, so the binaries are downloaded by
`make warm-postgres` before the tests start rather than by the first test.
"""

from __future__ import annotations

import tomllib
import typing as typ

import pytest
from ci_workflow_reader import REPO_ROOT, repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

WARM_COMMAND: typ.Final = "make warm-postgres"
PROFILE: typ.Final = "postgres"
URL_VARIABLE: typ.Final = "POSTGRES_TEST_URL"

# Each job that runs the PostgreSQL tests, with the name of the step that runs
# them and the profile expression that step must carry.
POSTGRES_TEST_STEPS: typ.Final = (
    (
        "ci.yml",
        "build-test",
        "Test",
        "${{ matrix.name == 'postgres' && 'postgres' || 'default' }}",
    ),
    ("ci.yml", "coverage", "Generate coverage for Postgres", PROFILE),
    ("coverage-main.yml", "coverage-upload", "Generate coverage for Postgres", PROFILE),
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


def _scalars(value: object) -> cabc.Iterator[str]:
    """Every key and string value in a parsed document, depth first."""
    if isinstance(value, dict):
        for key, item in value.items():
            yield str(key)
            yield from _scalars(item)
    elif isinstance(value, list):
        for item in value:
            yield from _scalars(item)
    elif isinstance(value, str):
        yield value


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
        if any(URL_VARIABLE in scalar for scalar in _scalars(document))
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
    ("workflow", "job_id", "step_name", "profile"),
    POSTGRES_TEST_STEPS,
    ids=[f"{w}:{j}" for w, j, _, _ in POSTGRES_TEST_STEPS],
)
def test_the_postgres_tests_run_serialized_after_the_download(
    workflow: str,
    job_id: str,
    step_name: str,
    profile: str,
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """The profile is on the step, and the warm-up runs earlier in the job."""
    steps = _steps(_jobs(documents[workflow])[job_id])
    positions = [i for i, step in enumerate(steps) if step.get("name") == step_name]
    assert len(positions) == 1, f"expected one {step_name!r} step in {job_id}"
    (test_at,) = positions
    env = steps[test_at].get("env")
    assert isinstance(env, dict) and env.get("NEXTEST_PROFILE") == profile, (
        f"{workflow}:{job_id} {step_name!r} must set NEXTEST_PROFILE to {profile!r}"
    )
    warm_at = [i for i, step in enumerate(steps) if step.get("run") == WARM_COMMAND]
    assert len(warm_at) == 1 and warm_at[0] < test_at, (
        f"{workflow}:{job_id} must run {WARM_COMMAND!r} once, before {step_name!r}"
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
