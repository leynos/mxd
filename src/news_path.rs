//! Utilities for traversing news bundle paths.
//!
//! This module constructs SQL fragments used in recursive CTEs to walk the
//! hierarchical bundle and category structure stored in the database.
//! It also provides helpers for preparing path parameters and executing the
//! recursive queries via Diesel.
// Generate the recursive CTE step SQL used for traversing news bundle paths.
//
// The `$jt` argument specifies the join type (`"JOIN"` or `"LEFT JOIN"`) used
// when linking the current tree node to the `news_bundles` table. The resulting
// SQL selects the next bundle in the path by comparing the segment at
// `tree.idx` to the bundle name.
cfg_if::cfg_if! {
    if #[cfg(feature = "postgres")] {
        macro_rules! step_sql {
            ($jt:literal) => {
                concat!(
                    "SELECT tree.idx + 1 AS idx, b.id AS id\n",
                    "FROM tree\n",
                    "JOIN json_each($1) seg ON seg.key = tree.idx\n",
                    $jt,
                    " news_bundles b ON b.name = seg.value AND\n  ((tree.id IS NULL AND \
                     b.parent_bundle_id IS NULL) OR b.parent_bundle_id = tree.id)"
                )
            };
        }

        pub(crate) const BUNDLE_BODY_SQL: &str = "SELECT id FROM tree WHERE idx = $1";
        pub(crate) const CATEGORY_BODY_SQL: &str = concat!(
            "SELECT c.id AS id \n",
            "FROM news_categories c \n",
            "JOIN json_each($1) seg ON seg.key = $2 \n",
            "WHERE c.name = seg.value AND c.bundle_id IS NOT DISTINCT FROM (SELECT id FROM tree WHERE idx = $3)"
        );
    } else {
        macro_rules! step_sql {
            ($jt:literal) => {
                concat!(
                    "SELECT tree.idx + 1 AS idx, b.id AS id\n",
                    "FROM tree\n",
                    "JOIN json_each(?) seg ON seg.key = tree.idx\n",
                    $jt,
                    " news_bundles b ON b.name = seg.value AND\n  ((tree.id IS NULL AND \
                     b.parent_bundle_id IS NULL) OR b.parent_bundle_id = tree.id)"
                )
            };
        }

        pub(crate) const BUNDLE_BODY_SQL: &str = "SELECT id FROM tree WHERE idx = ?";
        pub(crate) const CATEGORY_BODY_SQL: &str = concat!(
            "SELECT c.id AS id \n",
            "FROM news_categories c \n",
            "JOIN json_each(?) seg ON seg.key = ? \n",
            "WHERE c.name = seg.value AND c.bundle_id IS (SELECT id FROM tree WHERE idx = ?)"
        );
    }
}

pub(crate) const CTE_SEED_SQL: &str = "SELECT 0 AS idx, NULL AS id";
pub(crate) const BUNDLE_STEP_SQL: &str = step_sql!("JOIN");
pub(crate) const CATEGORY_STEP_SQL: &str = step_sql!("LEFT JOIN");

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

use diesel::{query_builder::QueryFragment, sql_query};
use diesel_cte_ext::{
    RecursiveCTEExt,
    RecursiveParts,
    cte::{RecursiveBackend, WithRecursive},
};

/// Construct a recursive CTE for traversing bundle paths.
///
/// The connection type `C` is only used for backend inference; a concrete
/// connection value is unnecessary. The step and body parameters are generic
/// because their query types change once bindings are applied.
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
    C::with_recursive(
        "tree",
        &["idx", "id"],
        RecursiveParts::new(seed, step, body),
    )
}

/// Convenience wrapper that infers the connection type from `conn`.
pub fn build_path_cte_with_conn<C, Step, Body>(
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
    let _ = conn; // type inference only
    build_path_cte::<C, Step, Body>(step, body)
}

#[cfg(test)]
mod tests {
    use rstest::{fixture, rstest};

    use super::*;

    fn expected_step_sql(join_type: &str) -> String {
        let sql = r"SELECT tree.idx + 1 AS idx, b.id AS id
FROM tree
JOIN json_each({source}) seg ON seg.key = tree.idx
{join_type} news_bundles b ON b.name = seg.value AND
  ((tree.id IS NULL AND b.parent_bundle_id IS NULL) OR b.parent_bundle_id = tree.id)";
        let source = if cfg!(feature = "postgres") {
            "$1"
        } else {
            "?"
        };
        sql.replace("{source}", source)
            .replace("{join_type}", join_type)
    }

    #[fixture]
    #[allow(unused_braces)]
    fn expected_bundle_step_sql() -> String { expected_step_sql("JOIN") }

    #[fixture]
    #[allow(unused_braces)]
    fn expected_category_step_sql() -> String { expected_step_sql("LEFT JOIN") }

    #[rstest]
    fn bundle_step_sql_matches_expected(expected_bundle_step_sql: String) {
        assert_eq!(BUNDLE_STEP_SQL, expected_bundle_step_sql);
    }

    #[rstest]
    fn category_step_sql_matches_expected(expected_category_step_sql: String) {
        assert_eq!(CATEGORY_STEP_SQL, expected_category_step_sql);
    }

    #[test]
    fn bundle_body_sql_matches_expected() {
        let expected = if cfg!(feature = "postgres") {
            "SELECT id FROM tree WHERE idx = $1"
        } else {
            "SELECT id FROM tree WHERE idx = ?"
        };
        assert_eq!(BUNDLE_BODY_SQL, expected);
    }

    #[test]
    fn category_body_sql_matches_expected() {
        let expected = if cfg!(feature = "postgres") {
            "SELECT c.id AS id \nFROM news_categories c \nJOIN json_each($1) seg ON seg.key = $2 \
             \nWHERE c.name = seg.value AND c.bundle_id IS NOT DISTINCT FROM (SELECT id FROM tree \
             WHERE idx = $3)"
        } else {
            "SELECT c.id AS id \nFROM news_categories c \nJOIN json_each(?) seg ON seg.key = ? \
             \nWHERE c.name = seg.value AND c.bundle_id IS (SELECT id FROM tree WHERE idx = ?)"
        };
        assert_eq!(CATEGORY_BODY_SQL, expected);
    }

    #[test]
    fn prepare_path_empty() {
        assert!(prepare_path("").unwrap().is_none());
        assert!(prepare_path("/").unwrap().is_none());
    }

    #[test]
    fn prepare_path_single_segment() {
        let (json, len) = prepare_path("foo").unwrap().unwrap();
        assert_eq!(json, "[\"foo\"]");
        assert_eq!(len, 1);
    }

    #[test]
    fn prepare_path_trailing_slash() {
        let (json, len) = prepare_path("foo/").unwrap().unwrap();
        assert_eq!(json, "[\"foo\"]");
        assert_eq!(len, 1);
    }
}
