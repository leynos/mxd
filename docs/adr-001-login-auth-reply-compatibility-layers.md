# Architectural decision record (ADR) 001: Separate login authentication and reply augmentation

## Status

Superseded on 2026-02-05 by ADR-003 (Split login authentication and reply
augmentation).

## Date

2026-02-05.

## Context and problem statement

The login flow now needs to support additional client-specific behaviour.
Hotline 1.8.5 and 1.9 clients expect banner fields in the login reply, while
SynHX clients avoid them. Future work must also support HOPE extensions and
SynHX hashed authentication, which alter request validation and response
shaping. The current approach adds compatibility logic in routing and
transaction parsing, which risks blending request authentication concerns with
reply decoration and makes the flow harder to evolve as new clients and
extensions arrive.

We need a structure that keeps request authentication and reply shaping
separable, maintains the wireframe boundary for protocol quirks, and provides
clear hooks for extension-specific logic.

## Decision drivers

- Preserve a clear boundary between authentication logic and reply shaping.
- Keep protocol quirks in the wireframe adapter rather than the domain layer.
- Make it easy to add HOPE extension reply fields without editing core login
  handlers.
- Support SynHX hashed authentication workflows described in
  `../../synhx-client/hx_commands.c`.
- Maintain predictable testability for both request validation and reply
  shaping.

## Requirements

### Functional requirements

- Support multiple authentication methods keyed by client compatibility or
  negotiated extensions.
- Allow reply augmentation to add or remove fields based on client type and
  extensions.
- Preserve current login reply behaviour for Hotline 1.8.5, Hotline 1.9, and
  SynHX clients.

### Technical requirements

- Keep compatibility logic at the adapter boundary.
- Provide explicit interfaces for authentication and reply augmentation.
- Avoid new external dependencies.

## Options considered

### Option A: Keep login logic monolithic

Continue to evolve the existing login handler to cover authentication and reply
formatting. This minimises new abstractions but makes the login path harder to
reason about as more client-specific logic lands.

### Option B: Add ad-hoc hooks in routing

Use routing-level checks to branch on client compatibility, adding extra login
fields and pre-processing authentication when needed. This keeps changes local
but duplicates logic across routes and ties correctness to call ordering.

### Option C: Split authentication and reply augmentation

Introduce a dedicated authentication strategy layer to validate login requests
and a reply augmenter to decorate login replies. Client compatibility and
extension negotiation feed both layers, keeping request validation separate
from reply shaping.

| Topic                       | Option A          | Option B                    | Option C                |
| --------------------------- | ----------------- | --------------------------- | ----------------------- |
| Separation of concerns      | Weak              | Moderate                    | Strong                  |
| Extension support           | Hard to scale     | Fragile and order dependent | Explicit and testable   |
| Compatibility boundary      | Mixed             | Adapter only                | Adapter only            |
| Cognitive load              | High              | Medium                      | Medium                  |
| Future HOPE features        | Requires rewrites | Requires more hooks         | Add augmenter rules     |
| SynHX hashed authentication | Embedded          | Embedded in routing         | Strategy implementation |

_Table 1: Trade-offs between login handling options._

## Decision outcome / proposed direction

Adopt Option C. Introduce a login authentication strategy interface that owns
request validation and credential checking, and a reply augmenter interface
that adds or removes reply fields based on client compatibility and negotiated
extensions. The existing `ClientCompatibility` policy remains the classifier
and feeds both layers. HOPE extension fields become reply augmentation rules,
while SynHX hashed authentication lives in a dedicated authentication strategy.

## Goals and non-goals

Goals:

- Keep login request validation and reply shaping independent.
- Provide a clean, testable path to add HOPE reply elements.
- Support SynHX hashed authentication without changing unrelated login logic.

Non-goals:

- Redesign the full authentication subsystem beyond login flows.
- Define the final HOPE negotiation protocol in this ADR.
- Replace existing compatibility classification logic.

## Migration plan

1. Phase 1: Introduce `AuthStrategy` and `LoginReplyAugmenter` traits in the
   wireframe adapter, with a default strategy that preserves current behaviour.
2. Phase 2: Route login handling through the strategy interface and wrap login
   replies with the augmenter, keeping existing compatibility tests green.
3. Phase 3: Add a SynHX hashed authentication strategy and a HOPE-aware reply
   augmenter, alongside new tests that exercise both paths.

## Known risks and limitations

- Strategy selection must remain consistent with compatibility classification
  to avoid mismatched authentication and reply policies.
- Introducing two layers increases the number of call sites that must remain
  ordered correctly.
- HOPE extension semantics are not fully defined yet, so augmenter rules may
  need revision.

## Outstanding decisions

- How HOPE extensions are negotiated during handshake or login.
- Which wireframe fields represent HOPE extension metadata.
- The exact SynHX hashed authentication handshake stages and error mapping.

## Architectural rationale

This approach aligns with the hexagonal boundary by keeping protocol quirks in
adapter-facing code while separating authentication from reply shaping. It
reduces coupling in the login handler and gives future features a predictable
place to land, making the system easier to evolve while preserving current
compatibility behaviour.
