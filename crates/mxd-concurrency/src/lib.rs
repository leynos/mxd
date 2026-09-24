//! Shared-state kernels of the mxd server, model-checked with Loom.
//!
//! The server keeps two pieces of state that concurrent connection tasks
//! mutate: the presence table behind `mxd::presence::PresenceRegistry`, and
//! the task-keyed connection-context registry behind
//! `mxd::wireframe::connection`. Their logic lives here, in a crate with no
//! dependencies, so that it builds under `--cfg loom`. The `mxd` crate itself
//! cannot: Tokio compiles its networking out in that configuration.
//!
//! Production calls these kernels, and the Loom models in `tests/` call the
//! same kernels, so a model exercises the code the server runs rather than a
//! copy of it. What stays in `mxd` is the environment around them: the
//! process-wide `OnceLock`, Tokio task identifiers and task-local storage, and
//! protocol error mapping. See `docs/verification-strategy.md` for the
//! boundary and the bounds.

pub mod context;
pub mod presence;
mod sync;
