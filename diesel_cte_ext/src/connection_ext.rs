use diesel::query_builder::QueryFragment;

use crate::{
    builders,
    cte::{RecursiveBackend, WithRecursive},
};

/// Extension trait providing a convenient `with_recursive` method on
/// connection types.
///
/// The backend is inferred from the connection, so callers do not need to
/// specify it explicitly.
pub trait RecursiveCTEExt {
    /// Backend associated with the connection.
    type Backend: RecursiveBackend;

    /// Create a [`WithRecursive`] builder for this connection's backend.
    ///
    /// See [`builders::with_recursive`] for parameter details.
    fn with_recursive<Seed, Step, Body>(
        &self,
        cte_name: &'static str,
        columns: &'static [&'static str],
        seed: Seed,
        step: Step,
        body: Body,
    ) -> WithRecursive<Self::Backend, Seed, Step, Body>
    where
        Seed: QueryFragment<Self::Backend>,
        Step: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>;
}

#[cfg(feature = "sqlite")]
impl RecursiveCTEExt for diesel::sqlite::SqliteConnection {
    type Backend = diesel::sqlite::Sqlite;

    fn with_recursive<Seed, Step, Body>(
        &self,
        cte_name: &'static str,
        columns: &'static [&'static str],
        seed: Seed,
        step: Step,
        body: Body,
    ) -> WithRecursive<Self::Backend, Seed, Step, Body>
    where
        Seed: QueryFragment<Self::Backend>,
        Step: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
    {
        builders::with_recursive::<Self::Backend, _, _, _>(cte_name, columns, seed, step, body)
    }
}

#[cfg(feature = "postgres")]
impl RecursiveCTEExt for diesel::pg::PgConnection {
    type Backend = diesel::pg::Pg;

    fn with_recursive<Seed, Step, Body>(
        &self,
        cte_name: &'static str,
        columns: &'static [&'static str],
        seed: Seed,
        step: Step,
        body: Body,
    ) -> WithRecursive<Self::Backend, Seed, Step, Body>
    where
        Seed: QueryFragment<Self::Backend>,
        Step: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
    {
        builders::with_recursive::<Self::Backend, _, _, _>(cte_name, columns, seed, step, body)
    }
}

#[cfg(all(feature = "diesel-async", feature = "sqlite"))]
impl RecursiveCTEExt
    for diesel_async::sync_connection_wrapper::SyncConnectionWrapper<
        diesel::sqlite::SqliteConnection,
    >
{
    type Backend = diesel::sqlite::Sqlite;

    fn with_recursive<Seed, Step, Body>(
        &self,
        cte_name: &'static str,
        columns: &'static [&'static str],
        seed: Seed,
        step: Step,
        body: Body,
    ) -> WithRecursive<Self::Backend, Seed, Step, Body>
    where
        Seed: QueryFragment<Self::Backend>,
        Step: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
    {
        builders::with_recursive::<Self::Backend, _, _, _>(cte_name, columns, seed, step, body)
    }
}

#[cfg(all(feature = "diesel-async", feature = "postgres"))]
impl RecursiveCTEExt for diesel_async::AsyncPgConnection {
    type Backend = diesel::pg::Pg;

    fn with_recursive<Seed, Step, Body>(
        &self,
        cte_name: &'static str,
        columns: &'static [&'static str],
        seed: Seed,
        step: Step,
        body: Body,
    ) -> WithRecursive<Self::Backend, Seed, Step, Body>
    where
        Seed: QueryFragment<Self::Backend>,
        Step: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
    {
        builders::with_recursive::<Self::Backend, _, _, _>(cte_name, columns, seed, step, body)
    }
}
