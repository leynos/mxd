use diesel::backend::Backend;
use diesel::query_builder::{AstPass, Query, QueryFragment, QueryId};
use diesel::result::QueryResult;

/// Marker trait for backends that support `WITH RECURSIVE`.
pub trait RecursiveBackend: Backend {}

#[cfg(feature = "sqlite")]
impl RecursiveBackend for diesel::sqlite::Sqlite {}

#[cfg(feature = "postgres")]
impl RecursiveBackend for diesel::pg::Pg {}

/// Representation of a recursive CTE query.
#[derive(Debug, Clone)]
pub struct WithRecursive<DB: Backend, Seed, Step, Body> {
    pub cte_name: &'static str,
    pub columns: &'static [&'static str],
    pub seed: Seed,
    pub step: Step,
    pub body: Body,
    pub _marker: std::marker::PhantomData<DB>,
}

impl<DB, Seed, Step, Body> QueryId for WithRecursive<DB, Seed, Step, Body>
where
    DB: Backend + 'static,
    Seed: 'static,
    Step: 'static,
    Body: 'static,
{
    type QueryId = Self;
    const HAS_STATIC_QUERY_ID: bool = true;
}



impl<DB, Seed, Step, Body> Query for WithRecursive<DB, Seed, Step, Body>
where
    DB: Backend,
    Body: Query,
{
    type SqlType = <Body as Query>::SqlType;
}

impl<DB, Seed, Step, Body> QueryFragment<DB> for WithRecursive<DB, Seed, Step, Body>
where
    DB: Backend,
    Seed: QueryFragment<DB>,
    Step: QueryFragment<DB>,
    Body: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        out.push_sql("WITH RECURSIVE ");
        out.push_identifier(self.cte_name)?;
        if !self.columns.is_empty() {
            out.push_sql(" (");
            for (i, col) in self.columns.iter().enumerate() {
                if i != 0 {
                    out.push_sql(", ");
                }
                out.push_identifier(col)?;
            }
            out.push_sql(")");
        }
        out.push_sql(" AS (");
        self.seed.walk_ast(out.reborrow())?;
        out.push_sql(" UNION ALL ");
        self.step.walk_ast(out.reborrow())?;
        out.push_sql(") ");
        self.body.walk_ast(out.reborrow())
    }
}

impl<DB, Seed, Step, Body, Conn> diesel::query_dsl::RunQueryDsl<Conn>
    for WithRecursive<DB, Seed, Step, Body>
where
    DB: Backend,
    Conn: diesel::connection::Connection<Backend = DB>,
    Self: QueryFragment<DB> + QueryId + Query,
{
}
