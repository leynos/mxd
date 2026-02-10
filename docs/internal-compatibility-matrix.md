# Internal compatibility matrix (Hotline and SynHX)

This internal matrix records implemented compatibility behaviour for Hotline
and SynHX clients in the wireframe adapter. It is the source of truth for
release-note QA sign-off when validating roadmap item 1.5.3.

For release-note QA sign-off workflow details, use
`docs/release-notes-qa-sign-off.md`.

## Scope and status labels

- `Supported`: implemented and covered by automated tests.
- `Partial`: implemented only for listed transactions; other behaviour is still
  pending roadmap delivery.
- `Planned`: not implemented yet; included to prevent over-claiming support.

## Compatibility matrix

| Client profile                 | Detection markers                                                   | Status    | Supported scope                                                                                | Known deviations                                                          | Required toggles                                            | Evidence                                                                          |
| ------------------------------ | ------------------------------------------------------------------- | --------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Hotline 1.8.5                  | Handshake `sub_version != 2`; login field 160 in `151..=189`        | Partial   | Login handshake and login reply augmentation; text payloads may be auto-decoded if XOR-encoded | User-list and messaging parity remains pending later roadmap items        | None required for default login flow                        | `src/wireframe/compat_policy.rs`; `tests/features/wireframe_login_compat.feature` |
| Hotline 1.9                    | Handshake `sub_version != 2`; login field 160 in `>=190`            | Partial   | Login handshake and login reply augmentation; text payloads may be auto-decoded if XOR-encoded | User-list and messaging parity remains pending later roadmap items        | None required for default login flow                        | `src/wireframe/compat_policy.rs`; `tests/features/wireframe_login_compat.feature` |
| SynHX (non-XOR)                | Handshake `sub_version == 2`; login field 160 accepted              | Partial   | Login flow with SynHX banner-field omission semantics                                          | Field-level parity outside implemented transactions remains roadmap work  | None for non-XOR payloads                                   | `src/wireframe/compat_policy.rs`; `tests/features/wireframe_login_compat.feature` |
| SynHX (XOR text mode)          | Same as SynHX plus XOR heuristic triggers on text fields            | Partial   | XOR login, XOR message payload detection, XOR news post payload decoding/encoding              | XOR mode is heuristic until a reliable handshake signal exists            | Client-side `encode` toggle enabled when using XOR payloads | `src/wireframe/compat.rs`; `tests/features/wireframe_xor_compat.feature`          |
| Unknown / legacy login version | Handshake `sub_version != 2`; login field 160 below `151` or absent | Supported | Safety behaviour: login reply omits Hotline banner extras and remains protocol-valid           | Client is treated conservatively as unknown until explicit mapping exists | None                                                        | `src/wireframe/compat_policy.rs`; `tests/features/wireframe_login_compat.feature` |

## Behavioural guarantees captured by tests

- Hotline 1.8.5 and 1.9 replies include fields 161/162.
- SynHX replies omit fields 161/162.
- Unknown client versions omit fields 161/162.
- XOR-encoded login and news payloads are accepted when heuristics detect XOR.
- Plaintext message payloads do not accidentally enable XOR compatibility.

## Open compatibility gaps

- Session-presence and messaging parity for these client profiles depends on
  future roadmap items under sections 2.x and beyond.
- Kani proofs for XOR and sub-version invariants are tracked separately by
  roadmap item 1.5.4.
- Compatibility guardrail routing entrypoint work remains tracked by roadmap
  items 1.5.5 and 1.5.6.
