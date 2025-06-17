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

/// Implement [`RecursiveCTEExt`] for a connection type using the specified
/// backend.
///
/// This macro reduces boilerplate for each connection and backend
/// combination.
macro_rules! impl_recursive_cte_ext {
    (
        $(#[$attr:meta])* $conn:ty => $backend:ty
    ) => {
        $(#[$attr])*
        impl RecursiveCTEExt for $conn {
            type Backend = $backend;

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
                builders::with_recursive::<Self::Backend, _, _, _>(
                    cte_name, columns, seed, step, body,
                )
            }
        }
    };
}

impl_recursive_cte_ext!(
    #[cfg(feature = "sqlite")]
    diesel::sqlite::SqliteConnection => diesel::sqlite::Sqlite
);

impl_recursive_cte_ext!(
    #[cfg(feature = "postgres")]
    diesel::pg::PgConnection => diesel::pg::Pg
);

impl_recursive_cte_ext!(
    #[cfg(all(feature = "diesel-async", feature = "sqlite"))]
    diesel_async::sync_connection_wrapper::SyncConnectionWrapper<diesel::sqlite::SqliteConnection>
        => diesel::sqlite::Sqlite
);

impl_recursive_cte_ext!(
    #[cfg(all(feature = "diesel-async", feature = "postgres"))]
    diesel_async::AsyncPgConnection => diesel::pg::Pg
);
