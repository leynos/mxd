//! Utilities for traversing `file_nodes` paths inside the DB adapter.
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

/// Seed row for the recursive CTE: starts at depth zero with no parent node.
///
/// Produces a single row `(idx = 0, id = NULL)` that anchors the first step
/// of the path traversal.
pub(crate) const CTE_SEED_SQL: &str = "SELECT 0 AS idx, CAST(NULL AS INTEGER) AS id";
/// Recursive step SQL for the file-node path CTE.
///
/// Advances the traversal one path segment at a time by joining the current
/// CTE row against the JSON path array and then against `file_nodes` on name
/// and parent-id. Backend-specific: one query uses
/// `json_array_elements_text ... WITH ORDINALITY`; the other uses `json_each`.
pub(crate) const FILE_NODE_STEP_SQL: &str = step_sql!();

use diesel::{query_builder::QueryFragment, sql_query};
use diesel_cte_ext::{
    RecursiveCTEExt,
    RecursiveParts,
    builders,
    cte::{RecursiveBackend, WithRecursive},
};

/// Normalise a slash-delimited path string for use as a CTE parameter.
///
/// Trims leading and trailing `/` characters, splits the remainder on `/`,
/// and serialises the resulting segments as a JSON array alongside the
/// segment count. Returns `Ok(None)` when the path resolves to an empty root
/// (i.e., the input is empty or contains only slashes).
///
/// # Errors
///
/// Returns a [`serde_json::Error`] if JSON serialization of the path segments
/// fails.
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

#[cfg(test)]
mod tests {
    use super::prepare_path;

    #[test]
    fn prepare_path_returns_none_for_empty_string() {
        let path = prepare_path("").expect("empty path should serialize");
        assert_eq!(path, None);
    }

    #[test]
    fn prepare_path_returns_none_for_slash_only() {
        let path = prepare_path("/").expect("slash-only path should serialize");
        assert_eq!(path, None);
    }

    #[test]
    fn prepare_path_returns_none_for_multiple_slashes() {
        let path = prepare_path("///").expect("slash-only path should serialize");
        assert_eq!(path, None);
    }

    #[test]
    fn prepare_path_single_segment() {
        let (json, len) = prepare_path("Docs")
            .expect("single-segment path should serialize")
            .expect("single segment should produce path parameters");
        assert_eq!(len, 1);
        assert_eq!(json, r#"["Docs"]"#);
    }

    #[test]
    fn prepare_path_multi_segment() {
        let (json, len) = prepare_path("/Docs/Reports/2024")
            .expect("multi-segment path should serialize")
            .expect("multi-segment path should produce path parameters");
        assert_eq!(len, 3);
        assert_eq!(json, r#"["Docs","Reports","2024"]"#);
    }

    #[test]
    fn prepare_path_strips_leading_and_trailing_slashes() {
        let (json, len) = prepare_path("/Docs/guide.txt/")
            .expect("path with boundary slashes should serialize")
            .expect("non-root path should produce path parameters");
        assert_eq!(len, 2);
        assert_eq!(json, r#"["Docs","guide.txt"]"#);
    }

    #[test]
    fn prepare_path_segment_count_matches_json_array_length() {
        let path = "a/b/c/d/e";
        let (json, len) = prepare_path(path)
            .expect("path should serialize")
            .expect("non-root path should produce path parameters");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("path JSON should parse as strings");
        assert_eq!(parsed.len(), len);
    }
}
