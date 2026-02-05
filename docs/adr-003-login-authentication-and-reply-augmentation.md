# Architectural decision record (ADR) 003: Split login authentication and reply augmentation

## Status

Proposed.

## Date

2026-02-05.

## Context and problem statement

The login flow must support client-specific authentication and reply shaping.
Hotline clients expect banner fields, SynHX requires hashed authentication, and
future HOPE extensions will add more reply elements. The current implementation
keeps compatibility logic in the wireframe adapter, but the login handler still
combines authentication and reply concerns. A structure is required that
separates request validation from reply decoration, and that can be wired into
the compatibility guardrails described in ADR-002.

## Decision drivers

- Separate authentication logic from reply shaping to keep responsibilities
  clear.
- Keep protocol quirks and extension handling at the wireframe boundary.
- Support SynHX hashed authentication without rewriting the core login handler.
- Make HOPE reply fields straightforward to add and test.
- Avoid introducing new external dependencies.

## Requirements

### Functional requirements

- Support multiple login authentication strategies keyed by client
  compatibility or negotiated extensions.
- Allow reply augmentation to add or remove fields per client or extension.
- Preserve existing login reply behaviour for Hotline 1.8.5, Hotline 1.9, and
  SynHX clients.

### Technical requirements

- Remain within wireframe modules and avoid domain-layer coupling.
- Integrate with the guardrail routing entrypoint described in ADR-002.
- Keep the login reply augmentation ordering deterministic and testable.

## Options considered

### Option A: Extend the existing login handler

Continue to evolve the current login handler to manage both authentication and
reply shaping. This is low-effort but increases coupling and makes extensions
harder to reason about.

### Option B: Add ad-hoc hooks in routing

Add route-specific logic to pre-process authentication and post-process replies
based on compatibility hints. This keeps changes near the routing layer but
creates duplication and ordering risks.

### Option C: Introduce authentication strategies and reply augmenters

Create an `AuthStrategy` interface for authentication workflows and a
`LoginReplyAugmenter` for reply decoration. Select implementations based on
compatibility metadata, and wire them into the routing entrypoint described in
ADR-002.

| Topic                       | Option A      | Option B                    | Option C                |
| --------------------------- | ------------- | --------------------------- | ----------------------- |
| Separation of concerns      | Weak          | Moderate                    | Strong                  |
| Extension support           | Hard to scale | Fragile and order dependent | Explicit and testable   |
| Compatibility boundary      | Mixed         | Adapter only                | Adapter only            |
| Cognitive load              | High          | Medium                      | Medium                  |
| SynHX hashed authentication | Embedded      | Embedded in routing         | Strategy implementation |
| HOPE reply fields           | Inline edits  | Inline edits                | Augmenter rules         |

_Table 1: Trade-offs between login handling options._

## Decision outcome / proposed direction

Adopt Option C. Implement an `AuthStrategy` interface that encapsulates login
request validation and credential checking, and a `LoginReplyAugmenter` that
handles reply decoration. Both are orchestrated by the compatibility guardrails
in ADR-002, which ensures the hooks run in the correct order. This preserves
existing behaviour while making SynHX hashed authentication and HOPE reply
fields explicit and testable.

## Goals and non-goals

Goals:

- Isolate authentication workflows from reply augmentation.
- Provide a predictable place to add HOPE reply elements.
- Support SynHX hashed authentication without affecting unrelated clients.

Non-goals:

- Redesign the full authentication subsystem outwith login.
- Define the final HOPE negotiation protocol in this ADR.
- Replace the existing compatibility classifier.

## Migration plan

1. Phase 1: Implement `AuthStrategy` and `LoginReplyAugmenter` traits with a
   default strategy that preserves current login behaviour.
2. Phase 2: Wire the strategy and augmenter into the guardrail routing
   entrypoint from ADR-002, keeping current tests passing.
3. Phase 3: Add a SynHX hashed authentication strategy and a HOPE-aware reply
   augmenter, alongside new behavioural tests.

## Known risks and limitations

- Strategy selection must remain consistent with compatibility classification
  to avoid mismatched authentication and reply policies.
- Reply augmentation still needs to update frame sizing when payloads change.
- HOPE extension semantics may shift, requiring adjustments to augmenters.

## Outstanding decisions

- How HOPE extensions are negotiated during handshake or login.
- The error mapping for SynHX hashed authentication failures.
- Which compatibility signals determine strategy selection.

## Architectural rationale

Splitting authentication strategies from reply augmentation keeps the login
path easy to extend while preserving a clear wireframe boundary for client
quirks. Coupled with the guardrail entrypoint in ADR-002, it reduces the risk
of accidental bypasses and keeps protocol evolution localized and testable.
