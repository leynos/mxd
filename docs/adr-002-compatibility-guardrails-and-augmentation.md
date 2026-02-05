# Architectural decision record (ADR) 002: Compatibility guardrails and augmentation

## Status

Proposed.

## Date

2026-02-05.

## Context and problem statement

Compatibility behaviour now depends on handshake metadata and login versions.
`ClientCompatibility` is passed through `RouteContext`, which makes it possible
for future routes to bypass compatibility hooks accidentally. Upcoming work
adds HOPE extensions and SynHX hashed authentication, which increases the
number of client-specific behaviours and the risk of duplicate or inconsistent
logic. We need a structure that enforces a single path for compatibility hooks
and keeps augmentation (such as `LoginReplyAugmenter`) and authentication
strategies wired in the correct order.

## Decision drivers

- Prevent compatibility hooks from being skipped in new routes.
- Centralise client-specific behaviour behind a consistent interface.
- Support reply augmentation and authentication strategies without leaking
  quirks into the domain layer.
- Provide tests that detect missing hooks early.
- Preserve the wireframe adapter as the compatibility boundary.

## Requirements

### Functional requirements

- All wireframe routing must invoke request and reply compatibility hooks.
- Login reply augmentation must be called for successful login replies.
- Authentication strategies and augmenters must share compatibility context.

### Technical requirements

- Keep compatibility logic within wireframe modules.
- Avoid new external dependencies.
- Provide explicit entrypoints to reduce bypass risk.

## Options considered

### Option A: Keep routing as-is with manual hook usage

Continue passing `ClientCompatibility` and expect each route to call the
appropriate hooks. This is low-effort but relies on developer discipline and
code review to prevent bypasses.

### Option B: Add a compatibility layer without a routing entrypoint

Introduce a `CompatibilityLayer` with `on_request` and `on_reply` methods, but
leave routing entrypoints public. This improves centralisation but still allows
routes to skip the layer.

### Option C: Introduce a single routing entrypoint with compatibility guards

Make a `WireframeRouter` the only public entrypoint, embed a
`CompatibilityLayer` that orchestrates `AuthStrategy` and
`LoginReplyAugmenter`, and add a spy-based test to assert hook ordering.

| Topic                         | Option A | Option B | Option C      |
| ----------------------------- | -------- | -------- | ------------- |
| Bypass risk                   | High     | Medium   | Low           |
| Centralised compatibility     | Weak     | Strong   | Strong        |
| Hook order enforcement        | None     | Partial  | Test-enforced |
| Effort to implement           | Low      | Medium   | Medium        |
| Supports reply augmentation   | Manual   | Built-in | Built-in      |
| Supports auth strategy wiring | Manual   | Manual   | Built-in      |

_Table 1: Trade-offs between compatibility guardrail options._

## Decision outcome / proposed direction

Adopt Option C. Introduce a `WireframeRouter` as the only public entrypoint and
place a `CompatibilityLayer` inside it. The layer owns `AuthStrategy` and
`LoginReplyAugmenter` instances, invoking them in order as part of the request
and reply lifecycle. A spy-based test will assert that hooks are invoked for
login requests and successful login replies, preventing accidental bypasses.

## Goals and non-goals

Goals:

- Ensure compatibility hooks are always executed.
- Make `LoginReplyAugmenter` a first-class part of the reply lifecycle.
- Provide a single place to extend quirks for HOPE extensions and SynHX.

Non-goals:

- Redesign the login protocol itself.
- Introduce global registries outside the wireframe adapter.
- Replace existing compatibility classification logic.

## Migration plan

1. Phase 1: Introduce a `WireframeRouter` type and make existing routing
   functions `pub(crate)` so new routes must use the router.
2. Phase 2: Add a `CompatibilityLayer` that owns `ClientCompatibility`,
   `AuthStrategy`, and `LoginReplyAugmenter`, with explicit `on_request` and
   `on_reply` hooks.
3. Phase 3: Add a spy-based unit test that asserts hook ordering for login
   requests and replies, including `LoginReplyAugmenter` invocation.
4. Phase 4: Add a lightweight quirk registry (for example, a static mapping of
   transaction types to hook requirements) to document expectations and reduce
   duplication.

## Known risks and limitations

- Centralising hooks may require refactoring existing tests that call routing
  helpers directly.
- A router entrypoint can increase indirection if not named clearly.
- Hook ordering tests need careful design to avoid false positives.

## Outstanding decisions

- Naming and module location for `WireframeRouter` and `CompatibilityLayer`.
- Whether the quirk registry is compiled or kept as documentation-only.
- The specific data passed into the spy test (for example, login payload
  shapes).

## Architectural rationale

This approach enforces compatibility behaviour at the wireframe boundary while
preserving a clean separation between authentication strategies and reply
augmentation. It reduces coupling by making the compatibility lifecycle a
single, testable path and supports future HOPE extensions and SynHX hashed
authentication without duplicating logic across routes.
