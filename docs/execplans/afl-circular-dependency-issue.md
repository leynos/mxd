# Fix the AFL Docker build stage cycle

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: DRAFT

## Purpose / big picture

Restore the nightly AFL++ container build so
`docker build -t mxd-fuzz -f fuzz/Dockerfile .` succeeds again and the fuzzing
workflow can run instead of failing during image creation. Success is visible
in two places: the Docker image builds without the
`circular dependency detected on stage: builder` error, and the resulting image
contains an executable `/usr/local/bin/fuzz` binary that AFL++ can run.

## Constraints

- Keep the fix focused on the fuzzing container path. The plan may change
  `fuzz/Dockerfile`, closely related documentation, and fuzzing workflow
  references, but must not alter unrelated runtime or protocol code.
- Preserve the existing fuzz harness entrypoint contract:
  `afl-fuzz ... -- /usr/local/bin/fuzz @@`.
- Do not add new third-party dependencies or new container images.
- Keep the workflow compatible with the existing repository convention that
  fuzz builds currently use the debug-profile harness unless a deliberate
  follow-up plan changes that convention everywhere.
- Validation must include the documented documentation gates for Markdown
  changes and a real Docker build on a machine with a running Docker daemon.

## Tolerances (exception triggers)

- Scope: stop and escalate if the repair requires changes outside
  `fuzz/Dockerfile`, `docs/fuzzing.md`, and `.github/workflows/fuzz.yml`.
- Interface: stop and escalate if the harness path must change from
  `/usr/local/bin/fuzz`.
- Dependencies: stop and escalate if the fix would require a different base
  image, a new toolchain manager, or additional packages.
- Ambiguity: stop and escalate if the desired artefact profile is not clearly
  debug. The current repository state points to debug, but a release-profile
  shift would require coordinated changes beyond a minimal repair.
- Validation: stop and escalate if
  `docker build -t mxd-fuzz -f fuzz/Dockerfile .` still fails after one
  stage-layout fix and one artefact-path fix, because that indicates a second
  root cause.

## Risks

- Risk: fixing only the self-referential `COPY --from=builder` line could
  expose a second failure because the final stage currently copies
  `/mxd/fuzz/target/release/fuzz` even though `cargo afl build` builds the
  debug harness by default. Severity: high Likelihood: high Mitigation: treat
  the stage-cycle repair and artefact-path alignment as one atomic change, then
  validate the produced image contains the expected binary.

- Risk: `docs/fuzzing.md` currently describes sanitizers and automatic
  nightly installation that do not appear in `fuzz/Dockerfile`. Severity:
  medium Likelihood: medium Mitigation: update only the statements directly
  affected by the repair and record any remaining documentation drift as a
  follow-up if it is outside the minimal fix.

- Risk: this environment cannot run Docker because no daemon socket is
  available, so local validation here is limited to static inspection.
  Severity: medium Likelihood: high Mitigation: require a real `docker build`
  validation step in the plan and note the exact command and expected outcome.

## Progress

- [x] (2026-03-11 00:19Z) Inspected `fuzz/Dockerfile`,
  `.github/workflows/fuzz.yml`, and `docs/fuzzing.md`.
- [x] (2026-03-11 00:19Z) Identified the immediate root cause: a
  `COPY --from=builder` instruction appears before the builder stage ends, so
  Docker sees a stage copying from itself and aborts with a circular dependency
  error.
- [x] (2026-03-11 00:19Z) Identified a follow-on defect in the same file: the
  final stage copies `target/release/fuzz`, but the build step uses
  `cargo afl build` without `--release`, so the produced harness is under
  `target/debug/fuzz`.
- [x] (2026-03-11 00:19Z) Drafted this ExecPlan for review.
- [ ] Implement the Dockerfile repair and any tightly related documentation
  updates.
- [ ] Run Markdown gates and Docker validation on a host with a running Docker
  daemon.
- [ ] Confirm the nightly workflow can proceed past image build.

## Surprises & Discoveries

- Observation: the failing line in `fuzz/Dockerfile` is not the final-stage
  copy. It is a stray
  `COPY --from=builder /mxd/fuzz/target/debug/fuzz /usr/local/bin/fuzz` placed
  before the second `FROM`, which makes the builder stage depend on itself.
  Evidence: `fuzz/Dockerfile:13` appears after
  `RUN cargo afl build --manifest-path fuzz/Cargo.toml` and before the next
  `FROM`. Impact: the fix must first correct Dockerfile stage boundaries, not
  the AFL command itself.

- Observation: the repository already points to a debug artefact everywhere
  except the final-stage copy in `fuzz/Dockerfile`. Evidence: `docs/fuzzing.md`
  runs `cargo afl fuzz ... fuzz/target/debug/fuzz` and
  `.github/workflows/fuzz.yml` sets `BUILD_PROFILE: debug`. Impact: the minimal
  consistent repair is to copy the debug harness in the final image, not to
  switch the whole fuzz path to release.

- Observation: Docker validation cannot be executed in this workspace because
  `/var/run/docker.sock` is absent. Evidence: `docker build ...` fails here
  with `failed to connect to the docker API at unix:///var/run/docker.sock`.
  Impact: the plan must specify an external validation step instead of claiming
  local proof.

## Decision Log

- Decision: plan a minimal repair that keeps the current debug-profile fuzz
  artefact convention. Rationale: the self-cycle bug is the immediate failure,
  and the repository already documents and configures debug artefacts
  elsewhere. Switching to release would enlarge scope and create avoidable
  ambiguity. Date/Author: 2026-03-11 / Codex

- Decision: include the release/debug artefact mismatch in the same fix even
  though the reported CI failure stops earlier. Rationale: once the stage cycle
  is removed, the build would otherwise fail or produce the wrong image
  contents in the next step. Shipping only the first half of the repair would
  be knowingly incomplete. Date/Author: 2026-03-11 / Codex

## Outcomes & Retrospective

The investigation is complete and the implementation plan is ready, but the
repair has not been applied yet. The main lesson is that the failure message
accurately points to Dockerfile stage structure, yet static inspection also
shows a second defect that should be fixed in the same change to avoid a
predictable follow-on failure.

## Context and orientation

The relevant files are small and tightly coupled:

- `fuzz/Dockerfile` builds the AFL harness and assembles the runtime image.
- `.github/workflows/fuzz.yml` is the nightly GitHub Actions workflow that
  runs `docker build -t mxd-fuzz -f fuzz/Dockerfile .`.
- `docs/fuzzing.md` is the user-facing guide for the same flow.

Docker builds multi-stage images by naming stages with `FROM ... AS <name>` and
allowing later stages to copy files from earlier stages using
`COPY --from=<name>`. A stage cannot copy from itself. In the current
`fuzz/Dockerfile`, the `builder` stage contains a `COPY --from=builder`
instruction before the next `FROM`, so Docker detects a circular dependency and
aborts before any AFL-specific logic matters.

The same file also mixes debug and release artefact paths. The builder runs
`cargo afl build --manifest-path fuzz/Cargo.toml`, which produces
`/mxd/fuzz/target/debug/fuzz` unless `--release` is passed. The final stage
instead copies `/mxd/fuzz/target/release/fuzz`, which does not match the rest
of the repository. The workflow and documentation both point to the debug
binary, so the minimal safe fix is to make the final image copy the debug
binary from the completed builder stage.

## Plan of work

Stage A is confirmation and red-state capture. Re-open `fuzz/Dockerfile`,
`.github/workflows/fuzz.yml`, and `docs/fuzzing.md`, then record the failing
stage structure and the artefact mismatch as the two issues this change will
address. Do not edit any files until the reader can explain why Docker reports
a cycle.

Stage B is the Dockerfile repair. In `fuzz/Dockerfile`, remove the stray
intra-stage `COPY --from=builder` line and keep exactly two stages: one
`builder` stage that runs `make corpus` and `cargo afl build`, then one final
runtime stage that copies the built harness from the completed builder stage
into `/usr/local/bin/fuzz`. Align that copy path with the actual artefact
generated by `cargo afl build`, which is the debug binary unless the plan is
explicitly revised to adopt release everywhere.

Stage C is documentation and workflow alignment. Update `docs/fuzzing.md` only
where needed so the described container behaviour matches the repaired
Dockerfile. Re-check `.github/workflows/fuzz.yml`; if it already matches the
debug artefact convention, no workflow edit is needed. If any wording implies a
release build or a different harness path, correct it in the same change.

Stage D is validation. Run the Markdown gates required for documentation
changes. Then, on a machine with a live Docker daemon, build the image and
inspect the final container for `/usr/local/bin/fuzz`. Finally, rerun the fuzz
workflow manually or via a pull request so the build step demonstrably clears
the previous failure point.

Do not proceed from Stage B to Stage C unless the edited Dockerfile has a valid
two-stage structure. Do not consider Stage D complete unless both the
documentation gates and a real Docker build succeed.

## Concrete steps

From the repository root:

1. Inspect the current files before editing.

   ```bash
   sed -n '1,220p' fuzz/Dockerfile
   sed -n '1,220p' .github/workflows/fuzz.yml
   sed -n '1,220p' docs/fuzzing.md
   ```

   Expected observations:

   ```plaintext
   fuzz/Dockerfile contains COPY --from=builder before the second FROM
   docs/fuzzing.md references fuzz/target/debug/fuzz
   .github/workflows/fuzz.yml sets BUILD_PROFILE: debug
   ```

2. Edit `fuzz/Dockerfile` so it has one builder stage and one final runtime
   stage, with the final stage copying the debug harness from the completed
   builder stage.

3. If needed, edit `docs/fuzzing.md` so the container description matches the
   repaired Dockerfile and the debug harness convention.

4. Run Markdown validation with logged output.

   ```bash
   set -o pipefail
   make fmt | tee /tmp/afl-circular-dependency-make-fmt.log
   set -o pipefail
   make markdownlint | tee /tmp/afl-circular-dependency-markdownlint.log
   set -o pipefail
   make nixie | tee /tmp/afl-circular-dependency-nixie.log
   ```

   Expected result:

   ```plaintext
   All three commands exit 0
   ```

5. On a machine with Docker available, run the container build and a simple
   runtime check.

   ```bash
   docker build -t mxd-fuzz -f fuzz/Dockerfile .
   docker run --rm mxd-fuzz bash -lc 'test -x /usr/local/bin/fuzz'
   ```

   Expected result:

   ```plaintext
   docker build completes without "circular dependency detected on stage: builder"
   docker run exits 0
   ```

6. Optionally prove the GitHub Actions path by triggering the fuzz workflow or
   pushing the change through pull-request CI, then confirm the "Build fuzzing
   container" step passes.

## Validation and acceptance

Acceptance criteria:

- `fuzz/Dockerfile` no longer contains a self-referential stage copy.
- The final image copies the same harness profile that the repository already
  documents and expects, namely `fuzz/target/debug/fuzz`.
- `docker build -t mxd-fuzz -f fuzz/Dockerfile .` succeeds on a machine with
  Docker.
- `docker run --rm mxd-fuzz bash -lc 'test -x /usr/local/bin/fuzz'` exits 0.
- `make fmt`, `make markdownlint`, and `make nixie` pass after any Markdown
  edits.
- The GitHub Actions fuzz workflow clears the previous image-build failure.

Quality method:

- Static verification by inspecting the final Dockerfile stage layout.
- Documentation gates via Makefile targets.
- Real container build and runtime smoke test on a Docker-enabled host.
- Workflow confirmation in GitHub Actions.

## Idempotence and recovery

The file edits are idempotent: reapplying the corrected two-stage Dockerfile
does not change behaviour further. The validation commands are safe to rerun.
If the Docker build fails after the stage-layout fix, inspect whether the final
copy path still points at `target/release/fuzz`; correcting that path is the
first retry. If a documentation command reformats Markdown, rerun
`make markdownlint` and `make nixie` after `make fmt`.

## Artifacts and notes

Current failing Dockerfile fragment:

```plaintext
RUN cargo afl build --manifest-path fuzz/Cargo.toml

COPY --from=builder /mxd/fuzz/target/debug/fuzz /usr/local/bin/fuzz

FROM aflplusplus/aflplusplus:latest
COPY --from=builder /mxd/fuzz/target/release/fuzz /usr/local/bin/fuzz
```

Current workflow evidence:

```plaintext
env:
  FUZZ_HARNESS: /usr/local/bin/fuzz
  BUILD_PROFILE: debug
```

## Interfaces and dependencies

No new interfaces or dependencies are required. The repaired image must still
provide:

```plaintext
/usr/local/bin/fuzz
```

The only container stages required at the end of this plan are:

```plaintext
builder: compiles the AFL harness
final image: runs afl-fuzz with /usr/local/bin/fuzz
```

## Revision note

Initial draft created after tracing the reported GitHub Actions failure to a
self-referential `COPY --from=builder` in `fuzz/Dockerfile` and identifying a
second debug-versus-release artefact mismatch that should be fixed in the same
change. Remaining work is implementation and validation only.
