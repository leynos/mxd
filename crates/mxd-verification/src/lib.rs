//! Formal verification specifications and test harnesses for mxd.
//!
//! This crate contains TLA+ specifications, Stateright models, and Kani
//! harnesses that verify correctness-critical behaviour of the mxd server.
//! See `docs/verification-strategy.md` for the verification approach.
//!
//! # TLA+ specifications
//!
//! TLA+ specs live under `tla/` with a `.tla` and matching `.cfg` file per
//! subsystem. Run them locally with `make tlc` or via Docker with
//! `scripts/run-tlc.sh`.
//!
//! Current specifications:
//!
//! - `MxdHandshake.tla` — Models the handshake state machine and verifies timeout, error-code, and
//!   readiness invariants.
