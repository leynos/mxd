//! Shared file-node test bodies and `PostgreSQL` harness helpers.

mod additional;
mod constraints;
mod harness;
mod principal_cleanup;
mod shared;
mod shared_core;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
mod sqlite;

pub(super) use constraints::{
    RootFileNodeSpec,
    create_root_file_node_for_owner,
    create_test_user,
    reject_invalid_basenames_body,
    reject_self_parent_body,
    reject_unknown_principal_body,
};
pub(super) use harness::seed_download_permission;
#[cfg(feature = "postgres")]
pub(super) use harness::with_embedded_pg;
pub(super) use principal_cleanup::cleanup_on_principal_delete_body;
pub(super) use shared::{
    file_node_check_kind_constraint_body,
    nested_child_not_visible_without_explicit_grant_body,
    non_download_permission_does_not_grant_visibility_body,
    resolve_file_node_path_returns_none_for_missing_path_body,
};
pub(super) use shared_core::{
    file_node_acl_flow_body,
    grant_revocation_removes_visibility_body,
    group_acl_visibility_body,
    group_membership_removal_revokes_visibility_body,
    resolve_file_node_path_and_alias_body,
};
#[cfg(feature = "postgres")]
pub(super) use sqlite::visible_root_files_merge_body;
