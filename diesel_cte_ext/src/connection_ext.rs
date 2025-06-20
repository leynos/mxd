//! Extension trait exposing a `with_recursive` method on Diesel connections.
//!
//! This trait provides convenient access to [`builders::with_recursive`] with
//! backend inference from the connection type. Both synchronous and
//! asynchronous Diesel connections implement `RecursiveCTEExt`.

use diesel::query_builder::QueryFragment;

use crate::{
    builders::{self, RecursiveParts},
    columns::Columns,
    cte::{RecursiveBackend, WithCte, WithRecursive},
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
    #[doc(alias = "builders::with_recursive")]
    fn with_recursive<Cols, Seed, Step, Body, ColSpec>(
        cte_name: &'static str,
        columns: ColSpec,
        parts: RecursiveParts<Seed, Step, Body>,
    ) -> WithRecursive<Self::Backend, Cols, Seed, Step, Body>
    where
        Seed: QueryFragment<Self::Backend>,
        Step: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
        ColSpec: Into<Columns<Cols>>,
    {
        builders::with_recursive::<Self::Backend, Cols, _, _, _, _>(cte_name, columns, parts)
    }

    /// Create a [`WithCte`] builder for this connection's backend.
    fn with_cte<Cols, Cte, Body, ColSpec>(
        cte_name: &'static str,
        columns: ColSpec,
        cte: Cte,
        body: Body,
    ) -> WithCte<Self::Backend, Cols, Cte, Body>
    where
        Cte: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
        ColSpec: Into<Columns<Cols>>,
    {
        builders::with_cte::<Self::Backend, Cols, _, _, _>(cte_name, columns, cte, body)
    }
}

/// Blanket implementation of [`RecursiveCTEExt`] for synchronous Diesel
/// connections.
#[cfg(not(feature = "async"))]
impl<C> RecursiveCTEExt for C
where
    C: diesel::connection::Connection,
    C::Backend: RecursiveBackend,
{
    type Backend = C::Backend;
}

/// Blanket implementation of [`RecursiveCTEExt`] for asynchronous Diesel
/// connections.
#[cfg(feature = "async")]
impl<C> RecursiveCTEExt for C
where
    C: diesel_async::AsyncConnection,
    C::Backend: RecursiveBackend,
{
    type Backend = C::Backend;
}
