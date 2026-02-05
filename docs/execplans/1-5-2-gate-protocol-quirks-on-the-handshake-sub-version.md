# Gate login quirks on handshake sub-version

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md does not exist in this repository.

## Purpose / big picture

Ensure Hotline 1.8.5 and 1.9 clients receive the login reply fields they
expect, while SynHX clients avoid unsupported banner fields. Success is
observable when the new unit tests and `rstest-bdd` v0.4.0 behavioural
scenarios prove that login replies include fields 161/162 for Hotline 1.8.5 and
1.9 and omit them for SynHX, with documentation and roadmap entries updated
accordingly.

## Constraints

- Domain modules must remain free of wireframe imports and compatibility
  conditionals. Login quirks stay in the wireframe adapter boundary.
- Handshake sub-version `2` must be treated as SynHX. Hotline 1.8.5 vs 1.9
  derives from login field 160.
- Login replies must include fields 161/162 for Hotline 1.8.5 and 1.9, and omit
  them for SynHX.
- No new external dependencies. Escalate if a new crate is required.
- Every new module must start with a module-level `//!` comment.
- Files must remain under 400 lines; split modules if needed.
- Documentation uses en-GB-oxendict spelling and wraps prose at 80 columns.
- Use `rstest` for unit tests and `rstest-bdd` v0.4.0 for behavioural tests.
- Use `pg_embedded_setup_unpriv` as documented in
  `docs/pg-embed-setup-unpriv-users-guide.md` for Postgres-backed tests.
- Run `make check-fmt`, `make lint`, and `make test` before committing. If
  documentation changes, also run `make fmt`, `make markdownlint`, and
  `make nixie`.

## Tolerances (exception triggers)

- Scope: if more than 20 files change or net changes exceed 700 LOC, stop and
  escalate.
- Interface: if a public API outside `src/wireframe` or `src/transaction` must
  change, stop and escalate.
- Dependencies: if a new external dependency is required, stop and escalate.
- Iterations: if tests or lint still fail after two fix attempts, stop and
  escalate.
- Ambiguity: if multiple valid interpretations of the handshake sub-version or
  login field mapping exist, stop and present options with trade-offs.

## Risks

- Risk: handshake sub-version mapping is incomplete. Severity: medium.
  Likelihood: medium. Mitigation: treat `sub_version == 2` as SynHX and keep
  policy centralised so follow-on tasks can adjust in one place.
- Risk: Hotline 1.8.5/1.9 clients are not available for live testing.
  Severity: medium. Likelihood: high. Mitigation: rely on unit/BDD tests and
  document the need to validate with real clients later.
- Risk: login version field may arrive as 32-bit data. Severity: low.
  Likelihood: medium. Mitigation: parse both u16 and u32 and cover in tests.
- Risk: banner field defaults could diverge from client expectations.
  Severity: low. Likelihood: low. Mitigation: keep defaults minimal and
  document the chosen values.

## Progress

- [x] (2026-02-05 10:10Z) Review protocol docs and roadmap requirements for
  login compatibility gating.
- [x] (2026-02-05 10:10Z) Implement adapter-level `ClientCompatibility` policy
  and wire it into routing.
- [x] (2026-02-05 10:10Z) Add `FieldId` entries and update XOR-compatible text
  field list.
- [x] (2026-02-05 10:10Z) Add unit tests and `rstest-bdd` scenarios for login
  reply fields.
- [x] (2026-02-05 10:10Z) Update design docs, user guide, and roadmap.
- [x] (2026-02-05 10:10Z) Run quality gates and record outcomes.

## Surprises & discoveries

- Observation: SynHX uses handshake sub-version `2` (confirmed in the SynHX
  header). Impact: makes handshake sub-version a reliable SynHX gate.
- Observation: the Hotline 1.9 protocol document describes login reply fields
  161/162 for clients reporting version >= 151. Impact: both 1.8.5 and 1.9
  should receive banner fields.
- Observation: login version field can arrive as u16 or u32. Impact: parsing
  needs to handle both lengths without panicking.

## Decision log

- Decision: introduce `ClientCompatibility` in `src/wireframe/compat_policy.rs`
  to centralise handshake/login quirks. Rationale: keeps wireframe-specific
  policy at the adapter boundary and prevents domain leakage. Date/Author:
  2026-02-05 / Codex.
- Decision: gate banner fields 161/162 on handshake sub-version `2` (SynHX) and
  login version field 160 for Hotline 1.8.5 vs 1.9. Rationale: aligns with
  protocol docs and keeps 1.9 fallbacks intact. Date/Author: 2026-02-05 / Codex.
- Decision: default banner ID to `0` and server name to `mxd` when missing.
  Rationale: preserves existing behaviour while satisfying Hotline 1.9 field
  expectations. Date/Author: 2026-02-05 / Codex.
- Decision: accept both 16-bit and 32-bit login version payloads.
  Rationale: supports clients that encode the version differently. Date/Author:
  2026-02-05 / Codex.

## Outcomes & retrospective

Compatibility gating now keys off handshake sub-version and login version,
login replies include fields 161/162 for Hotline 1.8.5 and 1.9, and SynHX skips
those fields. Unit tests and `rstest-bdd` scenarios cover the happy paths and
edge cases. Documentation and the roadmap reflect the new policy. Follow-on
work still needs live Hotline 1.8.5/1.9 verification once clients are available.

## Context and orientation

Handshake metadata is captured in `src/wireframe/connection.rs` and is
propagated to the wireframe router via `src/server/wireframe/mod.rs`. Login
transactions are routed through `src/wireframe/routes/mod.rs`, which is where
payload decoding and reply augmentation live. `src/field_id.rs` defines numeric
field IDs. Tests live in `src/wireframe/compat_policy.rs` (unit) and
`tests/wireframe_login_compat.rs` plus
`tests/features/wireframe_login_compat.feature` (behavioural). Documentation
updates belong in `docs/design.md`, `docs/users-guide.md`, and
`docs/roadmap.md`.

## Plan of work

Stage A: confirm protocol mapping by reading the existing roadmap and the
Hotline 1.9 protocol document, then record the interpretation in the Decision
Log.

Stage B: add unit tests that classify clients by handshake sub-version and
login version, including u16 and u32 payloads. Add `rstest-bdd` scenarios that
send login requests and assert which reply fields are present for Hotline
1.8.5, Hotline 1.9, and SynHX.

Stage C: implement a `ClientCompatibility` policy that records handshake
metadata, parses the login payload, and augments login replies with banner
fields when required. Thread this policy through routing and middleware so it
is available where replies are constructed.

Stage D: update documentation and mark roadmap entry 1.5.2 complete. Run
formatting, lint, and test gates after using `pg_embedded_setup_unpriv` to
prepare local Postgres testing.

## Concrete steps

1. Review roadmap and protocol docs:

   - Read `docs/roadmap.md` and `../../hl-protocol-docs/HLProtocol-1-9.md`.
   - Note the handshake sub-version and login field requirements in the
     Decision Log.

2. Implement compatibility policy:

   - Add `src/wireframe/compat_policy.rs` with the `ClientCompatibility` type.
   - Thread `ClientCompatibility` through
     `src/server/wireframe/mod.rs` and `src/wireframe/routes/mod.rs`.

3. Update field IDs and XOR text list:

   - Add `FieldId::BannerId` and `FieldId::ServerName` in
     `src/field_id.rs`.
   - Add `FieldId::ServerName` to `src/wireframe/compat.rs`.

4. Add tests:

   - Unit tests in `src/wireframe/compat_policy.rs`.
   - Behavioural tests in `tests/wireframe_login_compat.rs` and
     `tests/features/wireframe_login_compat.feature`.

5. Update docs:

   - `docs/design.md` for the policy.
   - `docs/users-guide.md` for runtime behaviour.
   - `docs/roadmap.md` to mark 1.5.2 done and note the new approach.

6. Prepare Postgres tests:

   - Run:

     PG_VERSION_REQ="=16.4.0" \
     PG_RUNTIME_DIR="/var/tmp/pg-embedded-setup-unpriv/install" \
     PG_DATA_DIR="/var/tmp/pg-embedded-setup-unpriv/data" \
     PG_SUPERUSER="postgres" \
     PG_PASSWORD="postgres_pass" \
     cargo run --release --bin pg_embedded_setup_unpriv

7. Run quality gates from the repo root:

   - `make fmt`
   - `make markdownlint`
   - `make nixie`
   - `make check-fmt`
   - `make lint`
   - `make test`

Expected output: each command exits with status 0 and reports success.

## Validation and acceptance

Acceptance is met when:

- The new unit tests in `src/wireframe/compat_policy.rs` pass.
- `rstest-bdd` scenarios in `tests/wireframe_login_compat.rs` pass for Hotline
  1.8.5 (sub-version 0, version 151), Hotline 1.9 (sub-version 0, version 190),
  and SynHX (sub-version 2, version 190).
- `make check-fmt`, `make lint`, and `make test` succeed.
- Documentation and roadmap updates are present.

## Idempotence and recovery

All steps are re-runnable. If `pg_embedded_setup_unpriv` fails, remove the
directories under `/var/tmp/pg-embedded-setup-unpriv` and rerun the setup
command. If tests fail due to unavailable Postgres, confirm the embedded setup
completed and rerun `make test`.

## Artifacts and notes

Evidence of success should include:

- `make test` summary showing all tests passed.
- Presence of banner fields 161/162 in login replies for Hotline 1.8.5/1.9 in
  `tests/wireframe_login_compat.rs`.

## Interfaces and dependencies

New or changed interfaces:

- `crate::wireframe::compat_policy::ClientCompatibility`
- `crate::field_id::FieldId::{BannerId, ServerName}`
- `crate::wireframe::routes::RouteContext` now includes `client_compat`

No new external dependencies are introduced.
