//! Helpers for traversing news bundle paths.
//!
//! This module builds SQL snippets for recursive CTE queries that walk the
//! hierarchical news structure stored in the database.
/// Generate the recursive CTE step SQL used for traversing news bundle paths.
///
/// The `$jt` argument specifies the join type (`"JOIN"` or `"LEFT JOIN"`) used
/// when linking the current tree node to the `news_bundles` table.  The
/// resulting SQL selects the next bundle in the path by comparing the segment
/// at `tree.idx` to the bundle name.
macro_rules! step_sql {
    ($jt:literal) => {
        concat!(
            "SELECT tree.idx + 1 AS idx, b.id AS id \n",
            "FROM tree \n",
            "JOIN json_each(?) seg ON seg.key = tree.idx \n",
            $jt,
            " news_bundles b ON b.name = seg.value AND \n  ((tree.id IS NULL AND \
             b.parent_bundle_id IS NULL) OR b.parent_bundle_id = tree.id)"
        )
    };
}

pub(crate) const CTE_SEED_SQL: &str = "SELECT 0 AS idx, NULL AS id";
pub(crate) const BUNDLE_STEP_SQL: &str = step_sql!("JOIN");
pub(crate) const CATEGORY_STEP_SQL: &str = step_sql!("LEFT JOIN");
pub(crate) const BUNDLE_BODY_SQL: &str = "SELECT id FROM tree WHERE idx = ?";
pub(crate) const CATEGORY_BODY_SQL: &str = concat!(
    "SELECT c.id AS id \n",
    "FROM news_categories c \n",
    "JOIN json_each(?) seg ON seg.key = ? \n",
    "WHERE c.name = seg.value AND c.bundle_id IS (SELECT id FROM tree WHERE idx = ?)"
);

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
) -> WithRecursive<C::Backend, diesel::query_builder::SqlQuery, Step, Body>
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
) -> WithRecursive<C::Backend, diesel::query_builder::SqlQuery, Step, Body>
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
    use super::*;

    #[test]
    fn bundle_step_sql_matches_expected() {
        let expected = "SELECT tree.idx + 1 AS idx, b.id AS id \nFROM tree \nJOIN json_each(?) \
                        seg ON seg.key = tree.idx \nJOIN news_bundles b ON b.name = seg.value AND \
                        \n  ((tree.id IS NULL AND b.parent_bundle_id IS NULL) OR \
                        b.parent_bundle_id = tree.id)";
        assert_eq!(BUNDLE_STEP_SQL, expected);
    }

    #[test]
    fn category_step_sql_matches_expected() {
        let expected = "SELECT tree.idx + 1 AS idx, b.id AS id \nFROM tree \nJOIN json_each(?) \
                        seg ON seg.key = tree.idx \nLEFT JOIN news_bundles b ON b.name = \
                        seg.value AND \n  ((tree.id IS NULL AND b.parent_bundle_id IS NULL) OR \
                        b.parent_bundle_id = tree.id)";
        assert_eq!(CATEGORY_STEP_SQL, expected);
    }

    #[test]
    fn bundle_body_sql_matches_expected() {
        assert_eq!(BUNDLE_BODY_SQL, "SELECT id FROM tree WHERE idx = ?");
    }

    #[test]
    fn category_body_sql_matches_expected() {
        let expected = "SELECT c.id AS id \nFROM news_categories c \nJOIN json_each(?) seg ON \
                        seg.key = ? \nWHERE c.name = seg.value AND c.bundle_id IS (SELECT id FROM \
                        tree WHERE idx = ?)";
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
