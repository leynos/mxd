//! Typed column lists used when constructing recursive CTEs.
//!
//! [`Columns`] couples runtime column names with a compile-time tuple of Diesel
//! column types. Helper macros build these lists from individual column paths or
//! complete table definitions.

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

impl Columns<()> {
    /// Construct column names from a Diesel table definition.
    pub fn for_table<Tbl>() -> Columns<<Tbl as Table>::AllColumns>
    where
        Tbl: Table,
        <Tbl as Table>::AllColumns: ColumnNames,
    {
        Columns::<Tbl::AllColumns>::default()
    }
}

/// Helper trait yielding column name arrays for tuples of Diesel column types.
pub trait ColumnNames {
    /// Column names in query order.
    const NAMES: &'static [&'static str];
}

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

tuple_column_names!(A);
tuple_column_names!(A, B);
tuple_column_names!(A, B, C);
tuple_column_names!(A, B, C, D);
tuple_column_names!(A, B, C, D, E);
tuple_column_names!(A, B, C, D, E, F);
tuple_column_names!(A, B, C, D, E, F, G);
tuple_column_names!(A, B, C, D, E, F, G, H);

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
