//! Utilities for traversing `file_nodes` paths.
//!
//! This module mirrors the recursive-CTE approach used by the news path
//! helpers, but targets the hierarchical file-sharing namespace introduced in
//! roadmap item 3.1.1.

cfg_if::cfg_if! {
    if #[cfg(feature = "postgres")] {
        macro_rules! step_sql {
            () => {
                concat!(
                    "SELECT tree.idx + 1 AS idx, f.id AS id\n",
                    "FROM tree\n",
                    "JOIN json_array_elements_text($1::json) WITH ORDINALITY seg(value, idx)\n",
                    "  ON seg.idx::int = tree.idx + 1\n",
                    "JOIN file_nodes f ON f.name = seg.value AND\n",
                    "  ((tree.id IS NULL AND f.parent_id IS NULL) OR f.parent_id = tree.id::int)"
                )
            };
        }

        pub(crate) const FILE_NODE_BODY_SQL: &str = "SELECT id FROM tree WHERE idx = $2";
    } else {
        macro_rules! step_sql {
            () => {
                concat!(
                    "SELECT tree.idx + 1 AS idx, f.id AS id\n",
                    "FROM tree\n",
                    "JOIN json_each(?) seg ON seg.key = tree.idx\n",
                    "JOIN file_nodes f ON f.name = seg.value AND\n",
                    "  ((tree.id IS NULL AND f.parent_id IS NULL) OR f.parent_id = tree.id)"
                )
            };
        }

        pub(crate) const FILE_NODE_BODY_SQL: &str = "SELECT id FROM tree WHERE idx = ?";
    }
}

pub(crate) const CTE_SEED_SQL: &str = "SELECT 0 AS idx, CAST(NULL AS INTEGER) AS id";
pub(crate) const FILE_NODE_STEP_SQL: &str = step_sql!();

use diesel::{query_builder::QueryFragment, sql_query};
use diesel_cte_ext::{
    RecursiveCTEExt,
    RecursiveParts,
    builders,
    cte::{RecursiveBackend, WithRecursive},
};

pub(crate) fn prepare_path(path: &str) -> Result<Option<(String, usize)>, serde_json::Error> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    let len = parts.len();
    let json = serde_json::to_string(&parts)?;
    Ok(Some((json, len)))
}

/// Construct a recursive CTE for traversing file-node paths.
pub(crate) fn build_path_cte<C, Step, Body>(
    step: Step,
    body: Body,
) -> WithRecursive<C::Backend, (), diesel::query_builder::SqlQuery, Step, Body>
where
    C: RecursiveCTEExt,
    C::Backend: RecursiveBackend + diesel::backend::DieselReserveSpecialization,
    Step: QueryFragment<C::Backend>,
    Body: QueryFragment<C::Backend>,
{
    let seed = sql_query(CTE_SEED_SQL);
    builders::with_recursive::<C::Backend, (), _, _, _, _>(
        "tree",
        &["idx", "id"],
        RecursiveParts::new(seed, step, body),
    )
}

/// Convenience wrapper that infers the backend from `conn`.
pub(crate) fn build_path_cte_with_conn<C, Step, Body>(
    conn: &mut C,
    step: Step,
    body: Body,
) -> WithRecursive<C::Backend, (), diesel::query_builder::SqlQuery, Step, Body>
where
    C: RecursiveCTEExt,
    C::Backend: RecursiveBackend + diesel::backend::DieselReserveSpecialization,
    Step: QueryFragment<C::Backend>,
    Body: QueryFragment<C::Backend>,
{
    let _ = conn;
    build_path_cte::<C, Step, Body>(step, body)
}
