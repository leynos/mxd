//! Typed column lists used when constructing recursive CTEs.
//!
//! [`Columns`] couples runtime column names with a compile-time tuple of Diesel
//! column types. Helper macros build these lists from individual column paths or
//! complete table definitions. The provided tuple implementations cover up to
//! sixteen columns (`A` through `P`). Extend the [`tuple_column_names!`] macro
//! invocations if you need support for more columns.

use diesel::{Table, query_source::Column};

/// Runtime column names with associated type-level metadata.
#[derive(Debug, Clone, Copy)]
pub struct Columns<T> {
    /// Column names in query order.
    pub names: &'static [&'static str],
    _marker: core::marker::PhantomData<T>,
}

impl<T> Columns<T> {
    /// Wrap a slice of column names for use with a CTE.
    pub const fn raw(names: &'static [&'static str]) -> Self {
        Self {
            names,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<T> Default for Columns<T>
where
    T: ColumnNames,
{
    fn default() -> Self { Self::raw(T::NAMES) }
}

/// Compile-time check for [`ColumnNames`] implementations.
const fn assert_column_names_impl<T: ColumnNames>() {}

impl Columns<()> {
    /// Construct column names from a Diesel table definition.
    ///
    /// # Note
    ///
    /// This method only works for tables with up to 16 columns due to macro limitations in Diesel.
    /// Attempting to use this with tables having more than 16 columns will result in a compile-time
    /// error.
    pub fn for_table<Tbl>() -> Columns<<Tbl as Table>::AllColumns>
    where
        Tbl: Table,
        <Tbl as Table>::AllColumns: ColumnNames,
    {
        // Touch the helper to surface a clearer error if `ColumnNames` is missing.
        // Fails to compile for tables with more than 16 columns supported by the macro.
        let _ = assert_column_names_impl::<Tbl::AllColumns>;
        Columns::<Tbl::AllColumns>::default()
    }
}

impl From<&'static [&'static str]> for Columns<()> {
    fn from(names: &'static [&'static str]) -> Self { Self::raw(names) }
}

impl<const N: usize> From<&'static [&'static str; N]> for Columns<()> {
    fn from(names: &'static [&'static str; N]) -> Self { Self::raw(&names[..]) }
}

/// Helper trait yielding column name arrays for tuples of Diesel column types.
pub trait ColumnNames {
    /// Column names in query order.
    const NAMES: &'static [&'static str];
}

/// Implements [`ColumnNames`] for tuples of Diesel column types.
///
/// This macro is expanded below for tuples of up to sixteen columns. If you
/// need to support a larger tuple, simply extend the invocations using
/// additional identifiers.
macro_rules! tuple_column_names {
    ($($name:ident),+) => {
        impl<$($name),+> ColumnNames for ($($name,)+)
        where
            $($name: Column,)+
        {
            const NAMES: &'static [&'static str] = &[$($name::NAME),+];
        }
    };
}

// Support tuples up to 16 columns (A..P).
tuple_column_names!(A);
tuple_column_names!(A, B);
tuple_column_names!(A, B, C);
tuple_column_names!(A, B, C, D);
tuple_column_names!(A, B, C, D, E);
tuple_column_names!(A, B, C, D, E, F);
tuple_column_names!(A, B, C, D, E, F, G);
tuple_column_names!(A, B, C, D, E, F, G, H);
tuple_column_names!(A, B, C, D, E, F, G, H, I);
tuple_column_names!(A, B, C, D, E, F, G, H, I, J);
tuple_column_names!(A, B, C, D, E, F, G, H, I, J, K);
tuple_column_names!(A, B, C, D, E, F, G, H, I, J, K, L);
tuple_column_names!(A, B, C, D, E, F, G, H, I, J, K, L, M);
tuple_column_names!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
tuple_column_names!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
tuple_column_names!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

#[macro_export]
macro_rules! columns {
    ($($col:path),+ $(,)?) => {
        $crate::columns::Columns::<($($col,)+)>::default()
    };
}

#[macro_export]
macro_rules! table_columns {
    ($table:path) => {
        $crate::columns::Columns::for_table::<$table>()
    };
}
