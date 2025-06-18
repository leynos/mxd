use diesel::query_builder::QueryFragment;

use crate::cte::{RecursiveBackend, WithRecursive};

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

pub fn with_recursive<DB, Seed, Step, Body>(
    cte_name: &'static str,
    columns: &'static [&'static str],
    parts: RecursiveParts<Seed, Step, Body>,
) -> WithRecursive<DB, Seed, Step, Body>
where
    DB: RecursiveBackend,
    Seed: QueryFragment<DB>,
    Step: QueryFragment<DB>,
    Body: QueryFragment<DB>,
{
    WithRecursive {
        cte_name,
        columns,
        seed: parts.seed,
        step: parts.step,
        body: parts.body,
        _marker: std::marker::PhantomData,
    }
}
