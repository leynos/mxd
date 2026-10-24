"""Every PostgreSQL test in CI runs against an embedded cluster.

The postgres leg once ran against a `postgres:15` service container reached
through `POSTGRES_TEST_URL`, so the embedded path the suite is written for
never ran in CI at all. Both are gone, and these contracts keep them gone:
no job declares a service container, no workflow sets the URL, and no Rust
source reads it.

The embedded path brings obligations of its own, asserted here per job that
runs the PostgreSQL tests. Each process generates its own superuser password
unless `PG_PASSWORD` fixes one, and a cluster directory left by one process
then refuses the next, so each such job pins it. A cluster bootstrap failure
is cached for the rest of a test process, so the binaries are downloaded by
`make warm-postgres` before the tests start rather than by the first test.
Each job also pins the PostgreSQL bundle, because the harness version alone
does not choose a working one; see the bundle-pin contract for why.

The tests themselves run in parallel. pg-embed-setup-unpriv 0.6 gives every
cluster its own data directory, so the single-slot test group that serialized
the 0.5.x runs is gone, and a contract keeps it gone.
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
TOKEN_EXPRESSION: typ.Final = "${{ github.token }}"
PROFILE: typ.Final = "postgres"
URL_VARIABLE: typ.Final = "POSTGRES_TEST_URL"
CRATE: typ.Final = "pg-embed-setup-unpriv"


class PostgresTestStep(typ.NamedTuple):
    """A job that runs the PostgreSQL tests, and what its test step must set.

    Attributes
    ----------
    workflow : str
        File name of the workflow under `.github/workflows`.
    job_id : str
        Identifier of the job that runs the PostgreSQL tests.
    step_name : str
        Name of the step inside that job that invokes the tests.
    profile : str
        The nextest profile the step must select, as a literal or expression.
    """

    workflow: str
    job_id: str
    step_name: str
    profile: str


POSTGRES_TEST_STEPS: typ.Final = (
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


@pytest.mark.parametrize(
    "expected",
    POSTGRES_TEST_STEPS,
    ids=[f"{step.workflow}:{step.job_id}" for step in POSTGRES_TEST_STEPS],
)
def test_the_warm_up_is_authenticated(
    expected: PostgresTestStep,
    documents: cabc.Mapping[str, cabc.Mapping[str, object]],
) -> None:
    """The warm-up sends the workflow token, or GitHub may refuse it with 403.

    postgresql_embedded lists releases through the GitHub API, which rate limits
    anonymous callers by runner address. A refusal fails the warm-up, and with
    it the only PostgreSQL run in the job.
    """
    where = f"{expected.workflow}:{expected.job_id}"
    job = _jobs(documents[expected.workflow])[expected.job_id]
    (warm,) = [s for s in _steps(job) if s.get("run") == WARM_COMMAND]
    env = warm.get("env")
    assert isinstance(env, dict) and env.get("GITHUB_TOKEN") == TOKEN_EXPRESSION, (
        f"{where} warm-up must set GITHUB_TOKEN to {TOKEN_EXPRESSION!r}, found {env!r}"
    )


def _nextest_config() -> dict[str, object]:
    """The parsed nextest configuration."""
    return tomllib.loads(
        (REPO_ROOT / ".config" / "nextest.toml").read_text(encoding="utf-8")
    )


def test_the_postgres_profile_does_not_serialize_the_tests() -> None:
    """No single-slot group, and no override that applies one to every test.

    pg-embed-setup-unpriv 0.5.x shared one data directory between every cluster
    in a run, so the profile had to run one test at a time. 0.6 gives each
    cluster its own directory, and the profile now runs at the default
    parallelism; a reintroduced group or `all()` override would silently undo
    that and hide the throughput the migration was for.
    """
    config = _nextest_config()
    groups = config.get("test-groups") or {}
    assert "pg-embed" not in groups, f"the serializing group returned: {groups}"
    overrides = config["profile"][PROFILE].get("overrides") or []
    wide = [o for o in overrides if o.get("filter") == "all()" and "test-group" in o]
    assert not wide, f"an override serializes every test again: {wide}"


def test_the_postgres_profile_keeps_its_time_budget() -> None:
    """The profile still exists, with the slow timeouts it had.

    Removing the serialization must not quietly take the cold-start headroom
    with it: a cold cluster start was measured at 156 s, well past the default
    60 s slow timeout.
    """
    config = _nextest_config()
    profile = config["profile"][PROFILE]
    assert profile["global-timeout"] == "60m", profile
    assert profile["slow-timeout"]["period"] == "300s", profile["slow-timeout"]


def test_every_postgres_job_pins_the_working_bundle(documents) -> None:
    """Each job that runs the PostgreSQL tests pins the bundle exactly.

    The harness version does not choose a working bundle. The October theseus
    rebuilds express the time-zone aliases as tar hard links, the
    `postgresql_archive` behind 0.6.3 skips hard links, and every diesel
    connection then fails with `invalid value for parameter "TimeZone": "UTC"`.
    The pin must be exact: the variable is a semver requirement and the highest
    match wins, so a range would select the broken release again.
    """
    for expected in POSTGRES_TEST_STEPS:
        where = f"{expected.workflow}:{expected.job_id}"
        job = _jobs(documents[expected.workflow])[expected.job_id]
        env = job.get("env")
        assert isinstance(env, dict), f"{where} has no job env"
        assert env.get("PG_VERSION_REQ") == "=18.6.0", (
            f"{where} must pin PG_VERSION_REQ exactly, found {env.get('PG_VERSION_REQ')!r}"
        )


def test_the_installed_helper_matches_the_locked_version(documents) -> None:
    """Both workflows install the crate version the lockfile holds.

    CI runs `pg_embedded_setup_unpriv` from the pinned binary for the warm-up
    while the tests link the crate from `Cargo.lock`. If the two disagree, the
    warm-up fills a cache for one version and the tests look for another.
    """
    lock = tomllib.loads((REPO_ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    versions = [p["version"] for p in lock["package"] if p["name"] == CRATE]
    assert len(versions) == 1, f"expected one {CRATE} in Cargo.lock, found {versions}"
    (locked,) = versions
    for workflow, document in documents.items():
        text = _text(document)
        if "pg-embed-setup-unpriv@" not in text:
            continue
        assert f"pg-embed-setup-unpriv@{locked}" in text, (
            f"{workflow} installs a helper version that Cargo.lock does not hold "
            f"({locked})"
        )
