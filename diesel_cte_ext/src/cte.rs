//! Core types modeling a recursive CTE query.
//!
//! [`WithRecursive`] represents the raw query fragment containing the CTE, and
//! [`RecursiveBackend`] marks Diesel backends that support recursive queries.

use diesel::{
    backend::Backend,
    query_builder::{AstPass, Query, QueryFragment, QueryId},
    result::QueryResult,
};

use crate::columns::Columns;

fn push_identifiers<DB, Cols>(
    out: &mut AstPass<'_, '_, DB>,
    cols: &Columns<Cols>,
) -> QueryResult<()>
where
    DB: Backend,
{
    let ids = cols.names;
    if ids.is_empty() {
        return Ok(());
    }
    out.push_sql(" (");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push_sql(", ");
        }
        out.push_identifier(id)?;
    }
    out.push_sql(")");
    Ok(())
}

/// Marker trait for backends that support `WITH RECURSIVE`.
pub trait RecursiveBackend: Backend {}

#[cfg(feature = "sqlite")]
impl RecursiveBackend for diesel::sqlite::Sqlite {}

#[cfg(feature = "postgres")]
impl RecursiveBackend for diesel::pg::Pg {}

/// Representation of a recursive CTE query.
#[derive(Debug, Clone)]
pub struct WithRecursive<DB: Backend, Cols, Seed, Step, Body> {
    pub cte_name: &'static str,
    pub columns: Columns<Cols>,
    pub seed: Seed,
    pub step: Step,
    pub body: Body,
    pub _marker: std::marker::PhantomData<DB>,
}

impl<DB, Cols, Seed, Step, Body> QueryId for WithRecursive<DB, Cols, Seed, Step, Body>
where
    DB: Backend + 'static,
    Cols: 'static,
    Seed: 'static,
    Step: 'static,
    Body: 'static,
{
    type QueryId = Self;
    const HAS_STATIC_QUERY_ID: bool = true;
}

impl<DB, Cols, Seed, Step, Body> Query for WithRecursive<DB, Cols, Seed, Step, Body>
where
    DB: Backend,
    Body: Query,
{
    type SqlType = <Body as Query>::SqlType;
}

impl<DB, Cols, Seed, Step, Body> QueryFragment<DB> for WithRecursive<DB, Cols, Seed, Step, Body>
where
    DB: Backend,
    Seed: QueryFragment<DB>,
    Step: QueryFragment<DB>,
    Body: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        out.push_sql("WITH RECURSIVE ");
        out.push_identifier(self.cte_name)?;
        push_identifiers(&mut out, &self.columns)?;
        out.push_sql(" AS (");
        self.seed.walk_ast(out.reborrow())?;
        out.push_sql(" UNION ALL ");
        self.step.walk_ast(out.reborrow())?;
        out.push_sql(") ");
        self.body.walk_ast(out.reborrow())
    }
}

impl<DB, Cols, Seed, Step, Body, Conn> diesel::query_dsl::RunQueryDsl<Conn>
    for WithRecursive<DB, Cols, Seed, Step, Body>
where
    DB: Backend,
    Conn: diesel::connection::Connection<Backend = DB>,
    Self: QueryFragment<DB> + QueryId + Query,
{
}

/// Representation of a non-recursive CTE query.
#[derive(Debug, Clone)]
pub struct WithCte<DB: Backend, Cols, Cte, Body> {
    pub cte_name: &'static str,
    pub columns: Columns<Cols>,
    pub cte: Cte,
    pub body: Body,
    pub _marker: std::marker::PhantomData<DB>,
}

impl<DB, Cols, Cte, Body> QueryId for WithCte<DB, Cols, Cte, Body>
where
    DB: Backend + 'static,
    Cols: 'static,
    Cte: 'static,
    Body: 'static,
{
    type QueryId = Self;
    const HAS_STATIC_QUERY_ID: bool = true;
}

impl<DB, Cols, Cte, Body> Query for WithCte<DB, Cols, Cte, Body>
where
    DB: Backend,
    Body: Query,
{
    type SqlType = <Body as Query>::SqlType;
}

impl<DB, Cols, Cte, Body> QueryFragment<DB> for WithCte<DB, Cols, Cte, Body>
where
    DB: Backend,
    Cte: QueryFragment<DB>,
    Body: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        out.push_sql("WITH ");
        out.push_identifier(self.cte_name)?;
        push_identifiers(&mut out, &self.columns)?;
        out.push_sql(" AS (");
        self.cte.walk_ast(out.reborrow())?;
        out.push_sql(") ");
        self.body.walk_ast(out.reborrow())
    }
}

impl<DB, Cols, Cte, Body, Conn> diesel::query_dsl::RunQueryDsl<Conn>
    for WithCte<DB, Cols, Cte, Body>
where
    DB: Backend,
    Conn: diesel::connection::Connection<Backend = DB>,
    Self: QueryFragment<DB> + QueryId + Query,
{
}
