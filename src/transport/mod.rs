//! Transport adapters for the Marrakesh Express daemon.
//!
//! This module groups the inbound networking adapters that drive the domain
//! logic. The legacy adapter keeps the bespoke Tokio listener used prior to
//! the Wireframe migration so integration tests and auxiliary tooling can
//! continue to exercise the protocol without depending on the future
//! `wireframe` binary.

pub mod legacy;
