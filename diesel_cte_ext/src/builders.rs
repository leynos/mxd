//! Helper types for constructing recursive CTE queries.
//!
//! [`with_recursive`] builds a [`WithRecursive`] query from a name, column list
//! and the [`RecursiveParts`] struct bundling the seed, step and body fragments.
//! These helpers are used indirectly via [`RecursiveCTEExt::with_recursive`].

use diesel::query_builder::QueryFragment;

use crate::{
    columns::Columns,
    cte::{RecursiveBackend, WithRecursive},
};

/// Query fragments used by a recursive CTE.
#[derive(Debug, Clone)]
pub struct RecursiveParts<Seed, Step, Body> {
    /// Seed query producing the first row(s) of the CTE.
    pub seed: Seed,
    /// Step query referencing the previous iteration's result.
    pub step: Step,
    /// Query consuming the CTE.
    pub body: Body,
}

impl<Seed, Step, Body> RecursiveParts<Seed, Step, Body> {
    /// Bundle the seed, step and body queries together.
    pub fn new(seed: Seed, step: Step, body: Body) -> Self { Self { seed, step, body } }
}

/// Build a recursive CTE query.

pub fn with_recursive<DB, Cols, Seed, Step, Body, ColSpec>(
    cte_name: &'static str,
    columns: ColSpec,
    parts: RecursiveParts<Seed, Step, Body>,
) -> WithRecursive<DB, Cols, Seed, Step, Body>
where
    DB: RecursiveBackend,
    Seed: QueryFragment<DB>,
    Step: QueryFragment<DB>,
    Body: QueryFragment<DB>,
    ColSpec: Into<Columns<Cols>>,
{
    WithRecursive {
        cte_name,
        columns: columns.into(),
        seed: parts.seed,
        step: parts.step,
        body: parts.body,
        _marker: std::marker::PhantomData,
    }
}
