"""What the AFL fuzz lane must keep doing for its run to mean anything.

The lane was red for months because three of its parts were never exercised
together: the image build, the run and the crash triage. Its tests live here
because each part is configuration or shell that a local run proves once and
nothing re-proves.

* The workflow contracts hold the run's flags: the stop signal that lets the
  upload steps run, the AFL variables a hosted runner needs, the baked-in
  corpus, and the triage step's entrypoint override.
* The Dockerfile contracts hold the pin, the build dependencies and the paths
  that the final image copies from.
* The triage script is run against stub ``afl-cmin`` and ``afl-tmin`` tools, so
  its own logic is tested without AFL: ``-C`` is passed, each reduced crash is
  minimized into ``unique``, and a missing directory, a non-executable harness
  or a failing tool is refused.

What AFL does with its arguments is AFL's behaviour and is out of scope.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess  # noqa: S404 - the triage script is run against stubs
import typing as typ
from pathlib import Path

import pytest
from ci_workflow_reader import REPO_ROOT, repository_documents

if typ.TYPE_CHECKING:
    import collections.abc as cabc

DOCKERFILE: typ.Final = REPO_ROOT / "fuzz" / "Dockerfile"
TRIAGE: typ.Final = REPO_ROOT / "scripts" / "triage_crashes.sh"
HARNESS: typ.Final = "/usr/local/bin/fuzz"
DIGEST_FROM: typ.Final = re.compile(
    r"^FROM\s+\S+@sha256:[0-9a-f]{64}(?:\s+AS\s+\w+)?$", re.MULTILINE
)

STUB_CMIN: typ.Final = """#!/bin/sh
echo "cmin $*" >> "$STUB_LOG"
[ "$STUB_FAIL" = cmin ] && exit 1
while [ $# -gt 0 ]; do
  case "$1" in -i) in=$2; shift 2 ;; -o) out=$2; shift 2 ;; *) shift ;; esac
done
cp "$in"/id* "$out"/
"""
STUB_TMIN: typ.Final = """#!/bin/sh
echo "tmin $*" >> "$STUB_LOG"
[ "$STUB_FAIL" = tmin ] && exit 1
while [ $# -gt 0 ]; do
  case "$1" in -i) in=$2; shift 2 ;; -o) out=$2; shift 2 ;; *) shift ;; esac
done
cp "$in" "$out"
"""


@pytest.fixture(scope="module")
def fuzz_steps() -> dict[str, dict[str, object]]:
    """The fuzz job's steps by name."""
    job = repository_documents()["fuzz.yml"]["jobs"]["fuzz"]  # type: ignore[index]
    return {step["name"]: step for step in job["steps"] if "name" in step}


@pytest.fixture(scope="module")
def dockerfile() -> str:
    """The fuzz image's Dockerfile text."""
    return DOCKERFILE.read_text()


def _run(steps: cabc.Mapping[str, dict[str, object]], name: str) -> str:
    """Return a step's script, collapsing continuations and runs of space."""
    joined = re.sub(r"\\\n\s*", " ", str(steps[name]["run"]))
    return re.sub(r"[ \t]+", " ", joined).strip()


def _job() -> dict[str, typ.Any]:
    """The fuzz job."""
    return repository_documents()["fuzz.yml"]["jobs"]["fuzz"]  # type: ignore[index]


def test_the_run_is_interrupted_for_the_duration_the_job_env_names(
    fuzz_steps: cabc.Mapping[str, dict[str, object]],
) -> None:
    """AFL runs until stopped, so the run is interrupted ahead of the ceiling."""
    run = _run(fuzz_steps, "Run AFL++")
    assert re.search(r'\btimeout --signal=INT "\$FUZZ_DURATION" docker run\b', run), run


def test_the_nightly_duration_is_shorter_than_the_job_ceiling() -> None:
    """A duration past the ceiling would cancel the job before triage and upload."""
    job = _job()
    duration = str(job["env"]["FUZZ_DURATION"])
    hours = re.search(r"\|\|\s*'(\d+)h'", duration)
    assert hours, f"no hour duration for scheduled runs in: {duration}"
    assert int(hours.group(1)) * 60 < int(job["timeout-minutes"]), duration


def test_a_pull_request_runs_the_lane_for_a_bounded_interval() -> None:
    """A pull request touching the lane runs it end to end, but briefly."""
    document = repository_documents()["fuzz.yml"]
    triggers = document.get("on", document.get(True))  # type: ignore[union-attr]
    assert "pull_request" in triggers, triggers
    paths = set(triggers["pull_request"]["paths"])
    assert {
        "fuzz/**",
        "scripts/triage_crashes.sh",
        ".github/workflows/fuzz.yml",
    } <= paths
    duration = str(_job()["env"]["FUZZ_DURATION"])
    assert re.search(r"pull_request' && '\d+s'", duration), duration


def test_a_run_that_never_started_fails_the_lane(
    fuzz_steps: cabc.Mapping[str, dict[str, object]],
) -> None:
    """Run AFL++ tolerates failure, so a separate step demands AFL's output."""
    assert fuzz_steps["Run AFL++"].get("continue-on-error") is True
    check = fuzz_steps["Check AFL++ ran"]
    assert "execs_done" in str(check["run"]), check
    assert "artifacts/main/fuzzer_stats" in str(check["run"]), check
    assert "if" not in check, check


def test_the_uploads_run_after_a_failed_check(
    fuzz_steps: cabc.Mapping[str, dict[str, object]],
) -> None:
    """The artefacts matter most when the lane failed."""
    for name in ("Triage crashes", "Upload crash corpus", "Upload full artefacts"):
        assert fuzz_steps[name].get("if") == "always()", name


def test_the_run_sets_the_afl_variables_a_hosted_runner_needs(
    fuzz_steps: cabc.Mapping[str, dict[str, object]],
) -> None:
    """The container is told to skip CPU scaling and core-pattern checks."""
    run = _run(fuzz_steps, "Run AFL++")
    for variable in ("AFL_SKIP_CPUFREQ=1", "AFL_I_DONT_CARE_ABOUT_MISSING_CRASHES=1"):
        assert f"-e {variable}" in run, f"{variable} missing from: {run}"


def test_the_run_uses_the_corpus_baked_into_the_image(
    fuzz_steps: cabc.Mapping[str, dict[str, object]],
) -> None:
    """A host mount at /corpus would shadow the seeds with an empty directory."""
    run = _run(fuzz_steps, "Run AFL++")
    assert ":/corpus" not in run, run
    assert ":/out" in run, run


def test_the_triage_step_overrides_the_afl_entrypoint(
    fuzz_steps: cabc.Mapping[str, dict[str, object]],
) -> None:
    """The entrypoint is afl-fuzz, so triage needs bash and the script as args."""
    run = _run(fuzz_steps, "Triage crashes")
    assert "--entrypoint bash" in run, run
    assert re.search(
        r'mxd-fuzz scripts/triage_crashes\.sh artifacts/main/crashes "\$FUZZ_HARNESS"',
        run,
    ), run
    assert "bash scripts/" not in run, run


def test_the_harness_path_the_workflow_triages_is_the_one_the_image_installs(
    dockerfile: str,
) -> None:
    """FUZZ_HARNESS names the file the final stage copies the binary to."""
    job_env = repository_documents()["fuzz.yml"]["jobs"]["fuzz"]["env"]  # type: ignore[index]
    assert job_env["FUZZ_HARNESS"] == HARNESS
    assert f"/mxd/target/debug/fuzz {HARNESS}" in dockerfile


def test_both_stages_are_pinned_to_the_same_digest(dockerfile: str) -> None:
    """A moving tag let the lane drift unnoticed; both stages must be pinned."""
    from_lines = DIGEST_FROM.findall(dockerfile)
    assert len(from_lines) == 2, f"expected two digest-pinned FROM lines: {from_lines}"
    digests = set(re.findall(r"sha256:[0-9a-f]{64}", dockerfile))
    assert len(digests) == 1, f"stages pin different digests: {digests}"


def test_the_builder_installs_what_the_base_image_lacks(dockerfile: str) -> None:
    """The base has neither libsqlite3 nor the `cargo afl` subcommand."""
    assert "libsqlite3-dev" in dockerfile
    assert re.search(r"cargo install cargo-afl --version \S+ --locked", dockerfile)


def test_cargo_afl_matches_the_afl_crate_the_harness_depends_on(
    dockerfile: str,
) -> None:
    """A cargo-afl and an afl crate on different minors disagree on the runtime."""
    manifest = (REPO_ROOT / "fuzz" / "Cargo.toml").read_text()
    crate = re.search(r'^afl = "(\d+\.\d+)', manifest, re.MULTILINE)
    tool = re.search(r"cargo-afl --version (\d+\.\d+)", dockerfile)
    assert crate, "fuzz/Cargo.toml has no afl dependency"
    assert tool, "the Dockerfile does not pin cargo-afl"
    assert crate.group(1) == tool.group(1), (crate.group(1), tool.group(1))


def test_the_afl_runtime_is_built_before_the_harness(dockerfile: str) -> None:
    """The runtime is per toolchain, so it is built after the toolchain is known."""
    config = dockerfile.find("cargo afl config --build")
    build = dockerfile.find("cargo afl build")
    copy = dockerfile.find("COPY . .")
    assert -1 < copy < config < build, (copy, config, build)


def test_the_final_image_carries_the_binary_and_the_seed_corpus(
    dockerfile: str,
) -> None:
    """The fuzz crate shares the workspace target dir, and the seeds ship inside."""
    assert "COPY --from=builder /mxd/target/debug/fuzz" in dockerfile
    assert "COPY --from=builder /mxd/fuzz/corpus /corpus" in dockerfile
    assert "fuzz/target/debug/fuzz" not in dockerfile


@pytest.fixture
def triage(tmp_path: Path) -> typ.Callable[..., subprocess.CompletedProcess[str]]:
    """Return a runner of the triage script against stub AFL tools."""
    stubs = tmp_path / "stubs"
    stubs.mkdir()
    for name, body in (("afl-cmin", STUB_CMIN), ("afl-tmin", STUB_TMIN)):
        (stubs / name).write_text(body)
        (stubs / name).chmod(0o755)
    harness = tmp_path / "harness"
    harness.write_text("#!/bin/sh\n")
    harness.chmod(0o755)

    def run(
        crash_dir: Path | None = None,
        *,
        fail: str = "",
        harness_path: Path | None = None,
    ) -> subprocess.CompletedProcess[str]:
        bash = shutil.which("bash")
        assert bash, "bash is required to run the triage script"
        env = {
            "PATH": f"{stubs}:{os.defpath}",
            "STUB_LOG": str(tmp_path / "stub.log"),
            "STUB_FAIL": fail,
        }
        target = crash_dir if crash_dir is not None else tmp_path / "crashes"
        return subprocess.run(  # noqa: S603 - fixed interpreter, repository script
            [bash, str(TRIAGE), str(target), str(harness_path or harness)],
            env=env,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )

    return run


def _crashes(tmp_path: Path, names: tuple[str, ...] = ("id:0", "id:1")) -> Path:
    """Create a crash directory holding one file per name."""
    crash_dir = tmp_path / "crashes"
    crash_dir.mkdir()
    for name in names:
        (crash_dir / name).write_bytes(b"\xff")
    return crash_dir


def test_triage_keeps_crashing_inputs_and_minimizes_each(
    tmp_path: Path, triage: typ.Callable[..., subprocess.CompletedProcess[str]]
) -> None:
    """afl-cmin gets -C, and every reduced crash lands in `unique`."""
    crash_dir = _crashes(tmp_path)
    result = triage(crash_dir)
    assert result.returncode == 0, result.stderr
    log = (tmp_path / "stub.log").read_text().splitlines()
    assert any(line.startswith("cmin -C ") for line in log), log
    assert sum(line.startswith("tmin ") for line in log) == 2, log
    assert sorted(p.name for p in (crash_dir / "unique").iterdir()) == ["id:0", "id:1"]


def test_triage_refuses_a_missing_crash_directory(
    tmp_path: Path, triage: typ.Callable[..., subprocess.CompletedProcess[str]]
) -> None:
    """No crash directory is an error, not an empty success."""
    result = triage(tmp_path / "absent")
    assert result.returncode == 1
    assert "does not exist" in result.stderr


def test_triage_refuses_a_harness_that_is_not_executable(
    tmp_path: Path, triage: typ.Callable[..., subprocess.CompletedProcess[str]]
) -> None:
    """A harness path that cannot run would make afl-cmin fail confusingly."""
    result = triage(_crashes(tmp_path), harness_path=tmp_path / "nope")
    assert result.returncode == 1
    assert "missing or not executable" in result.stderr


@pytest.mark.parametrize("tool", ["cmin", "tmin"])
def test_triage_fails_when_an_afl_tool_fails(
    tmp_path: Path,
    triage: typ.Callable[..., subprocess.CompletedProcess[str]],
    tool: str,
) -> None:
    """A failing reducer or minimizer fails the step rather than passing it."""
    result = triage(_crashes(tmp_path), fail=tool)
    assert result.returncode == 1
    assert f"afl-{tool} failed" in result.stderr
