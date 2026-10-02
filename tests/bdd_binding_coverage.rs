//! Ensure explicit async scenario bindings keep complete feature coverage.

use rstest::rstest;

#[rstest]
#[case::create_user(
    include_str!("features/create_user_command.feature"),
    include_str!("create_user_bdd.rs")
)]
#[case::handshake_metadata(
    include_str!("features/wireframe_handshake_metadata.feature"),
    include_str!("wireframe_handshake_metadata.rs")
)]
#[case::transaction_encoding(
    include_str!("features/wireframe_transaction_encoding.feature"),
    include_str!("wireframe_transaction_encoding/scenarios.rs")
)]
fn every_async_feature_scenario_has_a_named_binding(#[case] feature: &str, #[case] bindings: &str) {
    let mut declared: Vec<_> = feature
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Scenario:"))
        .map(str::trim)
        .collect();
    let mut bound: Vec<_> = bindings
        .lines()
        .filter_map(|line| line.trim().strip_prefix("name = \""))
        .filter_map(|name| name.split('"').next())
        .collect();
    assert!(!declared.is_empty(), "the feature must declare scenarios");
    declared.sort_unstable();
    bound.sort_unstable();
    assert_eq!(
        bound, declared,
        "every scenario needs exactly one async binding"
    );
}
