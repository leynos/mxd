"""Run the fuzz lane's shell logic against fixture directories.

``test_fuzz_lane`` reads the workflow as text. This module executes it: the
lane's checks, the crash gate and the archive step are taken from ``fuzz.yml``
and run under ``bash`` in a scratch directory holding AFL-shaped fixtures, and
the replay script is run against stub harnesses. Each is asserted on both
sides, so a gate that never fails, or a harness that does nothing, is caught.

AFL itself, and the built harness, are out of reach here; the lane's own run
covers the built harness, through the replay script these tests hold to account.
"""

from __future__ import annotations

import shutil
import subprocess  # noqa: S404 - the lane's shell logic is run against fixtures
import tarfile
import typing as typ
from pathlib import Path

import pytest
from ci_workflow_reader import REPO_ROOT, repository_documents

REPLAY: typ.Final = REPO_ROOT / "scripts" / "replay_harness.sh"
GOOD_STATS: typ.Final = "execs_done        : 4891\ncorpus_found      : 9\n"

# A stand-in harness: the three outcomes the real one prints, chosen by input.
STUB_HARNESS: typ.Final = """#!/bin/sh
f=$(mktemp); cat > "$f"
size=$(wc -c < "$f")
if [ "$size" -gt 1048596 ]; then echo Skipped >&2
elif [ "$size" -lt 20 ]; then echo Rejected >&2
else echo Accepted >&2; fi
rm -f "$f"
"""
NOOP_HARNESS: typ.Final = "#!/bin/sh\ncat > /dev/null\n"


def _step_script(name: str) -> str:
    """Return a step's `run` script from the fuzz job."""
    job = repository_documents()["fuzz.yml"]["jobs"]["fuzz"]  # type: ignore[index]
    return str(next(s["run"] for s in job["steps"] if s.get("name") == name))


def _run_step(name: str, cwd: Path) -> subprocess.CompletedProcess[str]:
    """Run a workflow step's script under bash as the runner does."""
    bash = shutil.which("bash")
    assert bash, "bash is required to run a workflow step"
    return subprocess.run(  # noqa: S603 - fixed interpreter, repository-owned script
        [bash, "-e", "-c", _step_script(name)],
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )


def _afl_output(
    root: Path, stats: str = GOOD_STATS, crashes: tuple[str, ...] = ()
) -> None:
    """Lay out AFL's output directory under `root/artifacts/main`."""
    main = root / "artifacts" / "main"
    (main / "crashes").mkdir(parents=True)
    (main / "fuzzer_stats").write_text(stats)
    (main / "crashes" / "README.txt").write_text("AFL readme")
    for name in crashes:
        (main / "crashes" / name).write_bytes(b"\xff")


def test_the_check_passes_a_run_that_executed_cases_and_found_paths(
    tmp_path: Path,
) -> None:
    """The healthy case passes, so the failures below are the check's doing."""
    _afl_output(tmp_path)
    assert _run_step("Check AFL++ ran", tmp_path).returncode == 0


@pytest.mark.parametrize(
    "stats",
    [
        pytest.param("execs_done        : 0\ncorpus_found      : 9\n", id="no-execs"),
        pytest.param(
            "execs_done        : 4891\ncorpus_found      : 0\n", id="no-paths"
        ),
        pytest.param("corpus_found      : 9\n", id="no-execs-line"),
    ],
)
def test_the_check_fails_a_run_that_did_nothing_useful(
    tmp_path: Path, stats: str
) -> None:
    """Zero executions, or none that reached a new path, fail the lane."""
    _afl_output(tmp_path, stats)
    assert _run_step("Check AFL++ ran", tmp_path).returncode != 0


def test_the_check_fails_when_afl_wrote_no_stats(tmp_path: Path) -> None:
    """A run that never started leaves no stats file."""
    (tmp_path / "artifacts").mkdir()
    assert _run_step("Check AFL++ ran", tmp_path).returncode != 0


def test_the_crash_gate_fails_on_a_saved_crash_and_passes_without(
    tmp_path: Path,
) -> None:
    """AFL's README is not a crash; an `id:*` file is."""
    clean = tmp_path / "clean"
    crashed = tmp_path / "crashed"
    _afl_output(clean)
    _afl_output(crashed, crashes=("id:000000,sig:06",))
    assert _run_step("Fail when AFL++ saved a crash", clean).returncode == 0
    result = _run_step("Fail when AFL++ saved a crash", crashed)
    assert result.returncode == 1
    assert "saved a crash" in result.stderr


def test_the_archive_step_packs_the_output_and_the_unique_crashes(
    tmp_path: Path,
) -> None:
    """Colons in AFL's names travel inside the archives, not as upload paths."""
    _afl_output(tmp_path, crashes=("id:000000,sig:06",))
    unique = tmp_path / "artifacts" / "main" / "crashes" / "unique"
    unique.mkdir()
    (unique / "id:000000,sig:06").write_bytes(b"\xff")
    assert _run_step("Archive the artefacts", tmp_path).returncode == 0
    with tarfile.open(tmp_path / "crashes.tgz") as crashes:
        assert crashes.getnames() == ["unique", "unique/id:000000,sig:06"]
    with tarfile.open(tmp_path / "fuzz-output.tgz") as everything:
        assert "./main/fuzzer_stats" in everything.getnames()


def test_the_archive_step_packs_an_empty_crash_set(tmp_path: Path) -> None:
    """A clean run still yields a valid, empty crash archive to upload."""
    _afl_output(tmp_path)
    assert _run_step("Archive the artefacts", tmp_path).returncode == 0
    with tarfile.open(tmp_path / "crashes.tgz") as crashes:
        assert crashes.getnames() == []


def _replay(tmp_path: Path, harness_body: str) -> subprocess.CompletedProcess[str]:
    """Run the replay script against a stub harness and a one-seed corpus."""
    harness = tmp_path / "harness"
    harness.write_text(harness_body)
    harness.chmod(0o755)
    corpus = tmp_path / "corpus"
    corpus.mkdir(exist_ok=True)
    (corpus / "seed.bin").write_bytes(b"\x00" * 40)
    bash = shutil.which("bash")
    assert bash, "bash is required to run the replay script"
    return subprocess.run(  # noqa: S603 - fixed interpreter, repository-owned script
        [bash, str(REPLAY), str(harness), str(corpus)],
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )


def test_replay_passes_a_harness_that_classifies_each_input(tmp_path: Path) -> None:
    """A seed is accepted, a malformed frame rejected and an oversized one skipped."""
    result = _replay(tmp_path, STUB_HARNESS)
    assert result.returncode == 0, result.stderr


def test_replay_fails_a_harness_that_does_nothing_with_its_input(
    tmp_path: Path,
) -> None:
    """AFL would still count its executions; the replay sees it said nothing."""
    result = _replay(tmp_path, NOOP_HARNESS)
    assert result.returncode == 1
    assert "nothing" in result.stderr


def test_replay_fails_a_harness_that_misclassifies(tmp_path: Path) -> None:
    """Treating a malformed frame as accepted is reported by name."""
    wrong = STUB_HARNESS.replace("echo Rejected", "echo Accepted")
    result = _replay(tmp_path, wrong)
    assert result.returncode == 1
    assert "expected Rejected" in result.stderr


def test_replay_refuses_a_missing_harness(tmp_path: Path) -> None:
    """An absent harness is an error, not a vacuous pass."""
    bash = shutil.which("bash")
    assert bash
    result = subprocess.run(  # noqa: S603 - fixed interpreter, repository script
        [bash, str(REPLAY), str(tmp_path / "absent"), str(tmp_path)],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    assert result.returncode == 1
    assert "missing or not executable" in result.stderr
