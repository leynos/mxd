use crate::cte::{RecursiveBackend, WithRecursive};

/// Build a recursive CTE query.
use diesel::query_builder::QueryFragment;

pub fn with_recursive<DB, Seed, Step, Body>(
    cte_name: &'static str,
    columns: &'static [&'static str],
    seed: Seed,
    step: Step,
    body: Body,
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
        seed,
        step,
        body,
        _marker: std::marker::PhantomData,
    }
}
