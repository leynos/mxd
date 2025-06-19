use diesel::{
    backend::Backend,
    query_builder::{AstPass, QueryFragment},
    result::QueryResult,
};

/// Wrapper for Diesel expressions used in recursive CTEs.
#[derive(Debug, Clone)]
pub struct QueryPart<T>(pub T);

impl<T> QueryPart<T> {
    /// Wrap a Diesel expression for use with `with_recursive`.
    pub fn new(expr: T) -> Self { Self(expr) }
}

impl<DB, T> QueryFragment<DB> for QueryPart<T>
where
    DB: Backend,
    T: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, out: AstPass<'_, 'b, DB>) -> QueryResult<()> { self.0.walk_ast(out) }
}

#[macro_export]
macro_rules! seed_query {
    ($expr:expr $(,)?) => {
        $crate::macros::QueryPart::new($expr)
    };
}

#[macro_export]
macro_rules! step_query {
    ($expr:expr $(,)?) => {
        $crate::macros::QueryPart::new($expr)
    };
}
