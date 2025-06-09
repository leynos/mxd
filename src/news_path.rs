macro_rules! step_sql {
    ($jt:literal) => {
        concat!(
            "SELECT tree.idx + 1 AS idx, b.id AS id \n",
            "FROM tree \n",
            "JOIN json_each(?) seg ON seg.key = tree.idx \n",
            $jt,
            " news_bundles b ON b.name = seg.value AND \n  ((tree.id IS NULL AND b.parent_bundle_id IS NULL) OR b.parent_bundle_id = tree.id)"
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

pub(crate) fn prepare_path(path: &str) -> Option<(String, usize)> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<String> = trimmed.split('/').map(|s| s.to_string()).collect();
    let len = parts.len();
    let json = serde_json::to_string(&parts).expect("serialize path segments");
    Some((json, len))
}
